#[cfg(feature = "host")]
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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
        BackendClient, BackendConfig, BackendEvent, BackendReply, INPUT_NOT_SENT, RemoteConnection,
        RemoteSession, TerminalControl, TerminalStatus,
    },
    protocol::{ConnectionState, SessionEntry, SessionKind, SessionStatus},
    terminal::{
        RemoteTerminal, SessionChange, Subscription, TERMINAL_INPUT_MAX_BYTES,
        TerminalPixelGeometry, TerminalSize,
    },
};
#[cfg(feature = "host")]
use crate::{network::LocalPublication, terminal::TerminalSession};

mod commands;
#[cfg(feature = "host")]
mod dosi;
#[cfg(feature = "host")]
mod restore;
#[cfg(test)]
mod tests;

#[cfg(feature = "host")]
type DosiMessage = crate::dosi::Message;
#[cfg(not(feature = "host"))]
type DosiMessage = std::convert::Infallible;

#[cfg(feature = "host")]
pub(crate) const NOT_PUBLISHED: &str =
    "Remote access is still connecting. Try again when this device is online and approved.";
pub(crate) const SHARE_ON_HOST: &str = "Change sharing on the computer that hosts this terminal.";
#[cfg(feature = "host")]
pub(crate) const SHARING_CHANGED: &str =
    "The sharing of this terminal changed. Open sharing again before you save.";
const MAX_SESSIONS: usize = 64;
const MAX_JOBS: usize = 64;
#[cfg(feature = "host")]
const MAX_PUBLICATIONS_IN_FLIGHT: usize = 8;
const COMMAND_CAPACITY: usize = 128;
const MAX_QUEUED_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_ANSWERS: usize = 256;
#[cfg(feature = "host")]
const CALL_ANSWER: Duration = Duration::from_secs(20);
#[cfg(feature = "host")]
const AGENT_ANSWER: Duration = Duration::from_secs(2);
const STARTING: Duration = Duration::from_secs(20);

type Observation = (Vec<Event>, broadcast::Receiver<Event>);

#[derive(Clone)]
pub struct RuntimeHandle {
    commands: mpsc::Sender<Request>,
    events: broadcast::Sender<Event>,
    stopped: watch::Receiver<bool>,
    clients: watch::Receiver<usize>,
    input_budget: Arc<Semaphore>,
    consumer: Arc<Consumer>,
}

struct Consumer {
    id: Uuid,
    lifetime: CancellationToken,
}

impl Default for Consumer {
    fn default() -> Self {
        Self {
            id: Uuid::now_v7(),
            lifetime: CancellationToken::new(),
        }
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.lifetime.cancel();
    }
}

enum Request {
    Command {
        envelope: CommandEnvelope,
        consumer: Uuid,
        lifetime: CancellationToken,
    },
    TerminalCommand {
        envelope: CommandEnvelope,
        connection: Uuid,
        reply: oneshot::Sender<Result<()>>,
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
        reply: Option<oneshot::Sender<Result<()>>>,
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
        self.send(Request::Command {
            envelope: command,
            consumer: self.consumer.id,
            lifetime: self.consumer.lifetime.clone(),
        })
    }

    #[cfg(feature = "host")]
    pub(crate) fn command_client(&self) -> Self {
        Self {
            consumer: Arc::new(Consumer::default()),
            ..self.clone()
        }
    }

    #[cfg(feature = "host")]
    fn answering(self) -> crate::network::Answering {
        Arc::new(move |request| {
            let handle = self.clone();
            Box::pin(async move { handle.answer_call(request).await })
        })
    }

    #[cfg(feature = "host")]
    async fn answer_call(&self, mut request: Value) -> Value {
        let asked = request["requestId"].take();
        let operation = request["type"].as_str().unwrap_or_default().to_owned();
        let own = Uuid::now_v7().to_string();
        request["requestId"] = json!(own);
        let mut answer = match self.call_command(request, &own).await {
            Ok(answer) => answer,
            Err(error) => {
                let area = operation.split('.').next().unwrap_or_default();
                json!({"type":format!("{area}.error"),"operation":operation,"message":error.to_string()})
            }
        };
        answer["requestId"] = asked;
        answer
    }

    #[cfg(feature = "host")]
    async fn call_command(&self, request: Value, own: &str) -> Result<Value> {
        let command: Command = serde_json::from_value(request)
            .map_err(|_| Error::Invalid(crate::protocol::OTHER_VERSION.to_owned()))?;
        command.validate()?;
        if !command.runs_on_other_computers() {
            return Err(Error::Invalid(
                "This computer takes only the choices and the start of a new terminal from your other devices."
                    .to_owned(),
            ));
        }
        let (events, mut receiver) = self.observe().await?;
        let scope = events.first().ok_or(Error::Stopped)?;
        self.try_send(CommandEnvelope {
            account_user_id: scope.account_user_id.clone(),
            account_epoch: scope.account_epoch,
            computer: None,
            command,
        })?;
        tokio::time::timeout(CALL_ANSWER, async {
            loop {
                match receiver.recv().await {
                    Ok(event)
                        if event.recipient == Some(self.consumer.id)
                            && event.event.request_id() == Some(own)
                            && event.event.is_answer() =>
                    {
                        return Ok(serde_json::to_value(&event.event)?);
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Err(Error::Stopped),
                }
            }
        })
        .await
        .map_err(|_| Error::Invalid("This computer did not finish in time.".to_owned()))?
    }

    pub async fn terminal_control(
        &self,
        envelope: CommandEnvelope,
        connection: Uuid,
    ) -> Result<Option<Value>> {
        if !matches!(
            envelope.command,
            Command::Resize { .. } | Command::Focus { .. } | Command::Blur { .. }
        ) || connection.is_nil()
        {
            return Err(Error::Invalid(
                "expected a subscribed terminal control".to_owned(),
            ));
        }
        let command = envelope.command.clone();
        let result = async {
            envelope.command.validate()?;
            let (reply, response) = oneshot::channel();
            self.send(Request::TerminalCommand {
                envelope,
                connection,
                reply,
            })?;
            response.await.map_err(|_| Error::Stopped)?
        }
        .await;
        Ok(crate::protocol::control_answer(&command, result))
    }

    pub fn consumer(&self) -> Uuid {
        self.consumer.id
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub fn local_clients(&self) -> watch::Receiver<usize> {
        self.clients.clone()
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
        let permit = self.input_permit(&bytes)?;
        self.send(Request::Input {
            session: session_id,
            incarnation: incarnation_id,
            connection: connection_id,
            bytes,
            permit,
            reply: None,
        })
    }

    pub async fn write_input(
        &self,
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
    ) -> Result<()> {
        let permit = self.input_permit(&bytes)?;
        let (reply, response) = oneshot::channel();
        self.send(Request::Input {
            session,
            incarnation,
            connection,
            bytes,
            permit,
            reply: Some(reply),
        })?;
        tokio::time::timeout(Duration::from_secs(5), response)
            .await
            .map_err(|_| {
                Error::Other("Input delivery was not confirmed; do not retry automatically.".into())
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

    fn input_permit(&self, bytes: &Bytes) -> Result<OwnedSemaphorePermit> {
        if bytes.len() > TERMINAL_INPUT_MAX_BYTES {
            return Err(Error::Invalid("terminal input exceeds 1 MiB".to_owned()));
        }
        let permits = u32::try_from(bytes.len()).map_err(|_| Error::Busy)?;
        Arc::clone(&self.input_budget)
            .try_acquire_many_owned(permits)
            .map_err(|_| Error::Busy)
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

#[cfg(feature = "host")]
struct Local {
    terminal: TerminalSession,
    entry: SessionEntry,
    creation: Command,
    published: bool,
    room_sequence: u64,
    worktree: Option<crate::worktree::Worktree>,
    wanted: Option<restore::Sharing>,
}

#[cfg(feature = "host")]
impl Local {
    fn stop(&self, keeps_worktree: bool) -> impl Future<Output = ()> + use<> {
        let terminal = self.terminal.clone();
        let worktree = self.worktree.clone().filter(|_| !keeps_worktree);
        async move {
            if terminal.close().await.is_err() {
                terminal.cancel();
            }
            if let Some(worktree) = worktree {
                crate::worktree::remove_unchanged(worktree).await;
            }
        }
    }

    fn comes_back(&self) -> bool {
        !self.terminal.is_closed() && self.entry.status != SessionStatus::Closing
    }

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
                mission_id: self
                    .entry
                    .mission_id
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok()),
                room_sequence: self.room_sequence,
                shared_with: self.entry.shared_with.iter().cloned().collect(),
            },
            self.terminal.host_requests.clone(),
            self.terminal.output(),
        )
    }
}

#[cfg(feature = "host")]
struct StartedSession {
    terminal: Option<TerminalSession>,
    directory: PathBuf,
    repository: bool,
    worktree: Option<crate::worktree::Worktree>,
}

#[cfg(feature = "host")]
impl Drop for StartedSession {
    fn drop(&mut self) {
        if let Some(terminal) = &self.terminal {
            terminal.cancel();
        }
        if let Some(worktree) = self.worktree.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(crate::worktree::remove_unchanged(worktree));
        }
    }
}

struct Opening {
    attempt: Uuid,
    cancellation: CancellationToken,
    commands: Vec<Command>,
}

struct Answer {
    consumer: Uuid,
    lifetime: CancellationToken,
}

fn answer(command: &Command, outcome: std::result::Result<(), &Error>) -> Value {
    let operation = command.operation();
    let area = operation.split('.').next().unwrap_or(operation);
    let mut value = match outcome {
        Ok(()) => {
            json!({"type":format!("{area}.result"), "requestId":command.request_id(), "operation":operation})
        }
        Err(error) => {
            json!({"type":format!("{area}.error"), "requestId":command.request_id(), "operation":operation, "message":error.to_string()})
        }
    };
    if let Some(session) = command.session_id() {
        value["sessionId"] = json!(session);
    }
    match command {
        Command::Room { room_id, .. } if outcome.is_ok() => value["roomId"] = json!(room_id),
        Command::RenameMission { mission_id, .. }
        | Command::DeleteMission { mission_id, .. }
        | Command::OpenMission { mission_id, .. }
        | Command::InviteMission { mission_id, .. }
        | Command::RemoveMissionMember { mission_id, .. }
        | Command::LeaveMission { mission_id, .. } => value["missionId"] = json!(mission_id),
        _ => {}
    }
    value
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
    #[cfg(feature = "host")]
    Created {
        id: Uuid,
        result: Result<StartedSession>,
    },
    BackendClient {
        command: Command,
        requested: bool,
        generation: u64,
        result: Result<BackendReply>,
    },
    Connected {
        id: Uuid,
        attempt: Uuid,
        result: Result<RemoteConnection>,
    },
    #[cfg(feature = "host")]
    Published {
        id: Uuid,
        incarnation: Uuid,
        refused: bool,
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
    Read {
        command: Command,
        result: Result<String>,
    },
    #[cfg(feature = "host")]
    Provider {
        command: Command,
        result: Result<Value>,
    },
    #[cfg(feature = "host")]
    Folders {
        command: Command,
        result: Result<crate::protocol::FolderList>,
    },
    Asked {
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
    #[cfg(feature = "host")]
    Local(TerminalSession),
    Remote(RemoteTerminal),
}

impl TerminalTarget {
    fn unsubscribe(&self, connection: Uuid) {
        match self {
            #[cfg(feature = "host")]
            Self::Local(terminal) => terminal.unsubscribe(connection),
            Self::Remote(terminal) => terminal.unsubscribe(connection),
        }
    }

    fn subscribe(&self) -> Pin<Box<dyn Future<Output = Result<Subscription>> + Send>> {
        match self {
            #[cfg(feature = "host")]
            Self::Local(terminal) => Box::pin(terminal.subscribe()),
            Self::Remote(terminal) => Box::pin(terminal.subscribe()),
        }
    }

    fn checkpoint(
        &self,
        connection: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<crate::network::CheckpointCut>> + Send>> {
        match self {
            #[cfg(feature = "host")]
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
            #[cfg(feature = "host")]
            Self::Local(terminal) => Box::pin(terminal.focus(connection, focused)),
            Self::Remote(terminal) => {
                let result = terminal.control(Some(connection), TerminalControl::Focus { focused });
                Box::pin(async move { result.await.map(|_| ()) })
            }
        }
    }

    fn read(&self, history: bool) -> Pin<Box<dyn Future<Output = Result<String>> + Send>> {
        match self {
            #[cfg(feature = "host")]
            Self::Local(terminal) => Box::pin(terminal.read(history)),
            Self::Remote(terminal) => Box::pin(terminal.read(history)),
        }
    }

    fn send_input(&self, bytes: Bytes) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        match self {
            #[cfg(feature = "host")]
            Self::Local(terminal) => Box::pin(terminal.send_input(bytes)),
            Self::Remote(terminal) => Box::pin(terminal.send_input(bytes)),
        }
    }

    fn end(&self, closing: bool) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        match self {
            #[cfg(feature = "host")]
            Self::Local(terminal) if closing => Box::pin(terminal.close()),
            #[cfg(feature = "host")]
            Self::Local(terminal) => Box::pin(terminal.interrupt()),
            Self::Remote(terminal) => {
                let result = terminal.control(
                    None,
                    if closing {
                        TerminalControl::Close
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
            #[cfg(feature = "host")]
            Self::Local(terminal) => Box::pin(terminal.resize(connection, size, geometry, claim)),
            Self::Remote(terminal) => {
                let result = terminal.control(
                    Some(connection),
                    TerminalControl::Resize {
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct Activity {
    working: bool,
    wait: Option<crate::terminal::ProgramState>,
}

impl Activity {
    fn of(entry: &SessionEntry) -> Self {
        use crate::terminal::ProgramState;
        let closing = entry.status == SessionStatus::Closing;
        let state = entry.program_status.as_ref().map(|status| status.state);
        let spinning = || {
            entry
                .title
                .as_deref()
                .is_some_and(crate::terminal::title_spins)
        };
        let working = !closing
            && state.map_or_else(
                || Self::connected(entry) && spinning(),
                |state| state == ProgramState::Working,
            );
        let wait = state.filter(|state| match state {
            ProgramState::Done | ProgramState::Error => true,
            ProgramState::Blocked => !closing,
            ProgramState::Idle | ProgramState::Working => false,
        });
        Self { working, wait }
    }

    fn connected(entry: &SessionEntry) -> bool {
        matches!(
            entry.connection_state,
            ConnectionState::Local | ConnectionState::Connected
        )
    }

    fn calls(entry: &SessionEntry) -> bool {
        entry.is_owner || Self::connected(entry)
    }
}

struct Reconnect {
    incarnation: Uuid,
    failures: u32,
    next: tokio::time::Instant,
    connected: Option<tokio::time::Instant>,
    failure: crate::diagnostics::Repeats,
}

struct Starting {
    answer: Value,
    until: tokio::time::Instant,
}

impl Reconnect {
    fn delay(failures: u32) -> Duration {
        Duration::from_millis(250 << failures.min(7))
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalNotice {
    session_id: Uuid,
    incarnation_id: Uuid,
    online: bool,
    status: Option<TerminalStatus>,
}

struct Runtime {
    host: Option<crate::HostKind>,
    #[cfg(feature = "host")]
    config: Config,
    #[cfg(feature = "host")]
    launcher: Arc<crate::agents::Launcher>,
    #[cfg(feature = "host")]
    dosi: crate::dosi::Engine,
    #[cfg(feature = "host")]
    recent: crate::folders::Recent,
    #[cfg(feature = "host")]
    restore: restore::Restore,
    network: BackendClient,
    events: broadcast::Sender<Event>,
    scope: Scope,
    #[cfg(feature = "host")]
    local: HashMap<Uuid, Local>,
    #[cfg(feature = "host")]
    creating: HashMap<Uuid, Command>,
    listed: HashMap<Uuid, RemoteSession>,
    remotes: HashMap<Uuid, SessionEntry>,
    starting: HashMap<Uuid, Starting>,
    connections: HashMap<Uuid, RemoteTerminal>,
    opening: HashMap<Uuid, Opening>,
    views: HashMap<Uuid, HashMap<Uuid, CancellationToken>>,
    visible: HashMap<Uuid, (CancellationToken, BTreeSet<String>)>,
    attention: BTreeSet<String>,
    activity: HashMap<String, Activity>,
    room_latest: HashMap<String, u64>,
    room_seen: Option<(String, BTreeMap<String, u64>)>,
    reconnect: HashMap<Uuid, Reconnect>,
    link_connects: u64,
    #[cfg(feature = "host")]
    publishing: BTreeSet<Uuid>,
    #[cfg(feature = "host")]
    retiring: BTreeSet<Uuid>,
    unpublishing: BTreeSet<Uuid>,
    administering: BTreeSet<Uuid>,
    jobs: JoinSet<Job>,
    changes: mpsc::Sender<SessionChange>,
    initializing: bool,
    auth_pending: bool,
    latest_auth: Option<Value>,
    cached_events: BTreeMap<String, Value>,
    answers: std::sync::Mutex<HashMap<String, Answer>>,
}

#[cfg(feature = "host")]
fn dosi_engine(
    config: &Config,
    launcher: &Arc<crate::agents::Launcher>,
) -> (crate::dosi::Engine, mpsc::Receiver<DosiMessage>) {
    crate::dosi::Engine::load(
        &config.data_root,
        &config.home,
        Arc::new(crate::dosi::Installed {
            launcher: Arc::clone(launcher),
            shell: commands::shell(config),
            home: config.home.clone(),
        }),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "makes every part of the runtime before its loop starts"
)]
pub async fn start(config: Config) -> Result<RuntimeHandle> {
    let network = BackendClient::new(BackendConfig {
        api_url: config.backend_url.clone(),
        issuer: config.oidc_issuer.clone(),
        client_id: config.oidc_client_id.clone(),
        scopes: config.oidc_scopes.clone(),
        data_root: config.data_root.clone(),
        secret_service: config.secret_service.clone(),
        isolated: config.isolated,
    })?;
    let network_events = network.events();
    let scope = Scope {
        user: None,
        epoch: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or_default()
            }),
        network_generation: network.generation(),
    };
    let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let (events, _) = broadcast::channel(256);
    let (changes, change_rx) = mpsc::channel(128);
    let (stopped, stopped_rx) = watch::channel(false);
    let (client_count, clients) = watch::channel(0);
    let handle = RuntimeHandle {
        commands,
        events: events.clone(),
        stopped: stopped_rx,
        clients,
        input_budget: Arc::new(Semaphore::new(MAX_QUEUED_INPUT_BYTES)),
        consumer: Arc::new(Consumer::default()),
    };
    #[cfg(feature = "host")]
    let server = crate::local_host::serve(
        handle.clone(),
        config.data_root.clone(),
        crate::local_host::HostDescription {
            pid: std::process::id(),
            kind: config.host,
        },
        client_count,
    )
    .await?;
    #[cfg(not(feature = "host"))]
    drop(client_count);
    #[cfg(feature = "host")]
    let launcher = Arc::new(crate::agents::Launcher::load(
        &config.data_root,
        &config.home,
        &commands::shell(&config),
    ));
    #[cfg(feature = "host")]
    let (dosi, dosi_messages) = dosi_engine(&config, &launcher);
    #[cfg(not(feature = "host"))]
    let (_, dosi_messages) = mpsc::channel::<DosiMessage>(1);
    #[cfg(feature = "host")]
    network.answer_calls(handle.command_client().answering());
    let runtime = Runtime {
        host: cfg!(feature = "host").then_some(config.host),
        #[cfg(feature = "host")]
        recent: crate::folders::Recent::load(&config.data_root),
        #[cfg(feature = "host")]
        restore: restore::Restore::load(&config.data_root),
        #[cfg(feature = "host")]
        config,
        #[cfg(feature = "host")]
        launcher,
        #[cfg(feature = "host")]
        dosi,
        network,
        events,
        scope,
        #[cfg(feature = "host")]
        local: HashMap::new(),
        #[cfg(feature = "host")]
        creating: HashMap::new(),
        listed: HashMap::new(),
        remotes: HashMap::new(),
        starting: HashMap::new(),
        connections: HashMap::new(),
        opening: HashMap::new(),
        views: HashMap::new(),
        visible: HashMap::new(),
        attention: BTreeSet::new(),
        activity: HashMap::new(),
        room_latest: HashMap::new(),
        room_seen: None,
        reconnect: HashMap::new(),
        link_connects: 0,
        #[cfg(feature = "host")]
        publishing: BTreeSet::new(),
        #[cfg(feature = "host")]
        retiring: BTreeSet::new(),
        unpublishing: BTreeSet::new(),
        administering: BTreeSet::new(),
        jobs: JoinSet::new(),
        changes,
        initializing: true,
        auth_pending: false,
        latest_auth: None,
        cached_events: BTreeMap::new(),
        answers: std::sync::Mutex::new(HashMap::new()),
    };
    tokio::spawn(async move {
        runtime
            .run(receiver, change_rx, network_events, dosi_messages)
            .await;
        #[cfg(feature = "host")]
        server.shutdown().await;
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
            Ok(mut event) => {
                if let Some(request) = event.event.request_id() {
                    let mut answers = self.answers();
                    let Some(answer) = answers.get(request) else {
                        tracing::debug!(
                            kind = event.event.kind(),
                            "an answer for a client that left was dropped"
                        );
                        return;
                    };
                    event.recipient = Some(answer.consumer);
                    if event.event.is_answer() {
                        answers.remove(request);
                    }
                }
                drop(self.events.send(event));
            }
            Err(error) => tracing::error!(%error, "invalid runtime event"),
        }
    }

    fn answers(&self) -> std::sync::MutexGuard<'_, HashMap<String, Answer>> {
        self.answers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn command(&mut self, envelope: &CommandEnvelope, consumer: Uuid, lifetime: CancellationToken) {
        let command = &envelope.command;
        let own = self.network.identity().map(|identity| identity.device_id);
        let elsewhere = envelope
            .computer
            .clone()
            .filter(|computer| Some(computer) != own.as_ref());
        if let Err(error) = self.expect_answer(command, consumer, lifetime.clone()) {
            self.refuse(command, consumer, &error);
        } else if envelope.account_epoch != self.scope.epoch
            || envelope.account_user_id != self.scope.user
        {
            self.error(command, &Error::Stale);
        } else if let Some(computer) = elsewhere {
            if let Err(error) = self.ask_computer(command, computer) {
                self.error(command, &error);
            }
        } else if let Err(error) = self.apply_for_consumer(command, consumer, lifetime) {
            self.error(command, &error);
        }
    }

    fn expect_answer(
        &self,
        command: &Command,
        consumer: Uuid,
        lifetime: CancellationToken,
    ) -> Result<()> {
        let request = command.request_id();
        let mut answers = self.answers();
        if answers.contains_key(request) {
            return Err(Error::Invalid(
                "That request is already in progress.".to_owned(),
            ));
        }
        if answers
            .values()
            .filter(|answer| answer.consumer == consumer)
            .count()
            >= MAX_ANSWERS
        {
            return Err(Error::Busy);
        }
        answers.insert(request.to_owned(), Answer { consumer, lifetime });
        drop(answers);
        Ok(())
    }

    fn refuse(&self, command: &Command, consumer: Uuid, error: &Error) {
        match self.event(answer(command, Err(error))) {
            Ok(mut event) => {
                event.recipient = Some(consumer);
                drop(self.events.send(event));
            }
            Err(error) => tracing::error!(%error, "invalid runtime event"),
        }
    }

    fn prune_answers(&mut self) {
        self.answers()
            .retain(|_, answer| !answer.lifetime.is_cancelled());
        self.visible
            .retain(|_, (lifetime, _)| !lifetime.is_cancelled());
    }

    fn entries(&self) -> Vec<SessionEntry> {
        let mut entries = self.remotes.values().cloned().collect::<Vec<_>>();
        #[cfg(feature = "host")]
        entries.extend(self.local.values().map(|local| local.entry.clone()));
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        for entry in &mut entries {
            entry.attention = self.attention.contains(&entry.id);
        }
        entries
    }

    fn publish_catalog(&mut self) {
        let entries = self.entries();
        let mut activity = HashMap::with_capacity(entries.len());
        for entry in &entries {
            let now = Activity::of(entry);
            let before = self.activity.get(&entry.id);
            let away = matches!(
                entry.status,
                SessionStatus::Offline | SessionStatus::Reconnecting
            );
            let stopped = !away && before.is_some_and(|before| before.working) && !now.working;
            let waits = now.wait.is_some() && before.map(|before| before.wait) != Some(now.wait);
            if (stopped || waits) && Activity::calls(entry) {
                self.notice(&entry.id);
            }
            activity.insert(entry.id.clone(), now);
        }
        self.activity = activity;
        self.attention.retain(|id| self.activity.contains_key(id));
        self.emit(json!({"type":"sessions.snapshot", "sessions":self.entries()}));
    }

    fn watched(&self, id: &str) -> bool {
        self.visible
            .values()
            .any(|(lifetime, ids)| !lifetime.is_cancelled() && ids.contains(id))
    }

    fn notice(&mut self, id: &str) {
        if self.watched(id) {
            return;
        }
        self.attention.insert(id.to_owned());
        self.emit(json!({"type":"term.attention", "sessionId":id}));
    }

    fn see(&mut self, consumer: Uuid, lifetime: CancellationToken, ids: BTreeSet<String>) {
        let seen = ids.iter().filter(|id| self.attention.remove(*id)).count();
        self.visible.insert(consumer, (lifetime, ids));
        if seen > 0 {
            self.publish_catalog();
        }
    }

    fn ready_event(&self) -> Value {
        json!({"type":"system.ready", "protocolVersion":crate::protocol::VERSION, "host":self.host})
    }

    fn snapshot_events(&self) -> Result<Vec<Event>> {
        let mut values = vec![
            self.ready_event(),
            self.auth_event(),
            json!({"type":"sessions.snapshot", "sessions":self.entries()}),
        ];
        #[cfg(feature = "host")]
        values.push(self.dosi.snapshot());
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
            Some(identity) => self
                .network
                .ready_event(&identity.user_id, identity.enrolled),
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
        mut network_events: broadcast::Receiver<BackendEvent>,
        mut dosi_messages: mpsc::Receiver<DosiMessage>,
    ) {
        let mut dosi_open = cfg!(feature = "host");
        self.emit(self.ready_event());
        let network = self.network.clone();
        self.spawn(async move {
            Completion::Initialized(network.initialize().await.map_err(Error::from))
        });
        let mut demand = tokio::time::interval(Duration::from_millis(100));
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
                        Some(Request::Command { envelope, consumer, lifetime }) => {
                            if !lifetime.is_cancelled() {
                                self.command(&envelope, consumer, lifetime);
                            }
                        }
                        Some(Request::TerminalCommand { envelope, connection, reply }) => {
                            if envelope.account_epoch != self.scope.epoch || envelope.account_user_id != self.scope.user {
                                drop(reply.send(Err(Error::Stale)));
                            } else {
                                self.terminal_command(&envelope.command, connection, reply);
                            }
                        }
                        Some(request) => self.request(request),
                    }
                }
                change = changes.recv() => {
                    if let Some(change) = change { self.change(change); }
                }
                message = dosi_messages.recv(), if dosi_open => {
                    match message {
                        Some(message) => self.dosi_message(message),
                        None => dosi_open = false,
                    }
                }
                event = network_events.recv() => {
                    match event {
                        Ok(event) => self.network_event(event),
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let internal = || Uuid::now_v7().to_string();
                            for command in [Command::RefreshAuth { request_id: internal() }, Command::RefreshDevices { request_id: internal() }, Command::RefreshFriends { request_id: internal() }, Command::ListMissions { request_id: internal() }, Command::ListSessions { request_id: internal() }] {
                                self.refresh(command);
                            }
                        },
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                job = self.jobs.join_next(), if !self.jobs.is_empty() => {
                    match job {
                        Some(Ok(job)) => self.complete(job),
                        Some(Err(error)) => {
                            tracing::error!(%error, "runtime operation failed");
                            self.emit(json!({"type":"system.error", "message":format!("Runtime operation failed: {error}")}));
                        }
                        None => {},
                    }
                }
                _ = demand.tick() => { self.prune_views(); self.prune_answers(); self.reconnect_views(); }
                _ = retry.tick() => { #[cfg(feature = "host")] { self.publish_locals(); self.retire_publications(); self.save_terminals(HashMap::new()); } self.reconnect_views(); self.started_remotes(); }
            }
        }
        requests.close();
        self.stop().await;
    }

    async fn stop(&mut self) {
        self.jobs.abort_all();
        while let Some(job) = self.jobs.join_next().await {
            #[cfg(feature = "host")]
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
            #[cfg(not(feature = "host"))]
            drop(job);
        }
        for opening in self.opening.values() {
            opening.cancellation.cancel();
        }
        for terminal in self.connections.values() {
            terminal.disconnect();
        }
        self.connections.clear();
        #[cfg(feature = "host")]
        self.dosi.shutdown();
        #[cfg(feature = "host")]
        self.save_terminals(self.agent_lines().await);
        #[cfg(feature = "host")]
        self.stop_locals().await;
        self.network.shutdown().await;
    }

    #[cfg(feature = "host")]
    fn dosi_message(&mut self, message: DosiMessage) {
        let mut out = crate::dosi::Output::default();
        self.dosi.handle(message, &mut out);
        self.dosi_output(out);
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer runs no Dosi")]
    const fn dosi_message(&self, message: DosiMessage) {
        match message {}
    }

    #[cfg(feature = "host")]
    async fn stop_locals(&self) {
        let mut stopping = JoinSet::new();
        for local in self.local.values() {
            stopping.spawn(local.stop(local.comes_back()));
        }
        while stopping.join_next().await.is_some() {}
    }

    /// Asks each terminal's agent which conversation it shows, so the conversation continues.
    #[cfg(feature = "host")]
    async fn agent_lines(&self) -> HashMap<Uuid, String> {
        let mut asking = JoinSet::new();
        for (id, local) in &self.local {
            if let Some(program) = local.entry.program.clone().filter(|_| local.comes_back()) {
                let (id, terminal, home) = (*id, local.terminal.clone(), self.config.home.clone());
                asking
                    .spawn(async move { (id, restore::agent_line(terminal, program, home).await) });
            }
        }
        let mut lines = HashMap::new();
        let answered = tokio::time::timeout(AGENT_ANSWER, async {
            while let Some(answer) = asking.join_next().await {
                if let Ok((id, Some(line))) = answer {
                    lines.insert(id, line);
                }
            }
        })
        .await;
        if answered.is_err() {
            tracing::warn!("an agent conversation was not found before the host stopped");
        }
        lines
    }

    #[cfg(feature = "host")]
    fn save_terminals(&mut self, mut lines: HashMap<Uuid, String>) {
        let mut ids = self
            .local
            .iter()
            .filter(|(_, local)| local.comes_back())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        let live = ids
            .into_iter()
            .map(|id| {
                let local = &self.local[&id];
                restore::Saved {
                    id,
                    name: local.entry.name.clone(),
                    folder: local.entry.working_dir.clone(),
                    sharing: local.wanted.clone().or_else(|| {
                        restore::Sharing::of(
                            local.entry.owner_user_id.as_ref(),
                            local.entry.mission_id.as_ref(),
                            &local.entry.shared_with,
                        )
                    }),
                    line: lines
                        .remove(&id)
                        .or_else(|| local.entry.program.as_deref().and_then(restore::usual_line)),
                    worktree: local.worktree.clone(),
                }
            })
            .collect();
        self.restore.save(live);
    }

    /// Starts the terminals of the last host again when an app asks.
    #[cfg(feature = "host")]
    pub(super) fn restore_saved(&mut self) {
        let saved = self.restore.take();
        if !saved.is_empty() {
            tracing::info!(
                terminals = saved.len(),
                "the terminals of the last host come back"
            );
        }
        for saved in saved {
            let command = Command::CreateSession {
                request_id: Uuid::now_v7().to_string(),
                name: saved.name.clone(),
                working_dir: restore::nearest_folder(saved.folder.as_deref()),
                mission_id: None,
                resume: None,
                branch: None,
                worktrees: None,
                command: saved.line.clone(),
                agent: None,
                prompt: None,
            };
            match self.begin_creation(command) {
                Ok(id) => {
                    self.restore.starting.insert(id, saved);
                }
                Err(error) => {
                    tracing::warn!(%error, name = %saved.name, "a terminal did not come back");
                }
            }
        }
    }

    /// Gives restored terminals their room or people when the account that chose them is signed in.
    #[cfg(feature = "host")]
    fn apply_wanted(&mut self) {
        let Some(user) = self.scope.user.clone() else {
            return;
        };
        for local in self.local.values_mut() {
            if let Some(sharing) = local.wanted.take_if(|sharing| sharing.owner == user) {
                local.entry.shared_with = if sharing.room.is_some() {
                    Vec::new()
                } else {
                    sharing.people
                };
                local.entry.mission_id = sharing.room;
                local.entry.mission_name = None;
            }
        }
    }

    fn clear_input_notice(&mut self, session: Uuid) {
        if let Some(entry) = self.remotes.get_mut(&session)
            && entry.message.as_deref() == Some(INPUT_NOT_SENT)
        {
            entry.message = None;
            self.publish_catalog();
        }
    }

    fn input(
        &mut self,
        session: Uuid,
        incarnation: Uuid,
        connection: Uuid,
        bytes: Bytes,
        permit: OwnedSemaphorePermit,
        reply: Option<oneshot::Sender<Result<()>>>,
    ) {
        let result = self
            .job_capacity()
            .and_then(|()| self.check_incarnation(session, incarnation))
            .and_then(|()| self.reachable(session))
            .and_then(|()| self.target(session));
        match result {
            Ok(target) => {
                let admission = match target {
                    #[cfg(feature = "host")]
                    TerminalTarget::Local(terminal) => {
                        terminal.write_input(connection, bytes).boxed()
                    }
                    TerminalTarget::Remote(terminal) => {
                        self.clear_input_notice(session);
                        terminal.write_input(connection, bytes).boxed()
                    }
                };
                self.spawn(async move {
                    let result = admission.await;
                    drop(permit);
                    match reply {
                        Some(reply) => Completion::HeadlessControl { reply, result },
                        None => Completion::Input { result },
                    }
                });
            }
            Err(error) => match reply {
                Some(reply) => drop(reply.send(Err(error))),
                None => self.emit(json!({"type":"system.error", "message":error.to_string()})),
            },
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
                let target = self
                    .job_capacity()
                    .and_then(|()| self.reachable(session))
                    .and_then(|()| self.target(session));
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
            Request::Input {
                session,
                incarnation,
                connection,
                bytes,
                permit,
                reply,
            } => self.input(session, incarnation, connection, bytes, permit, reply),
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
            Request::Command { .. } | Request::TerminalCommand { .. } | Request::Shutdown => {}
        }
    }

    fn target(&self, id: Uuid) -> Result<TerminalTarget> {
        #[cfg(feature = "host")]
        if let Some(local) = self.local.get(&id) {
            return Ok(TerminalTarget::Local(local.terminal.clone()));
        }
        self.connections
            .get(&id)
            .filter(|terminal| !terminal.is_closed())
            .cloned()
            .map(TerminalTarget::Remote)
            .ok_or(Error::NotFound)
    }

    #[cfg(feature = "host")]
    fn hosts(&self, id: &Uuid) -> bool {
        self.local.contains_key(id)
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer hosts no terminals")]
    const fn hosts(&self, _id: &Uuid) -> bool {
        false
    }

    fn check_incarnation(&self, id: Uuid, incarnation: Uuid) -> Result<()> {
        let expected = self
            .remotes
            .get(&id)
            .and_then(|entry| Uuid::parse_str(&entry.incarnation_id).ok());
        #[cfg(feature = "host")]
        let expected = self
            .local
            .get(&id)
            .map(|local| local.terminal.incarnation_id)
            .or(expected);
        if expected.ok_or(Error::NotFound)? == incarnation {
            Ok(())
        } else {
            Err(Error::Stale)
        }
    }

    fn error(&self, command: &Command, error: &Error) {
        if !matches!(error, Error::Stopped) {
            self.emit(answer(command, Err(error)));
        }
    }

    fn result(&self, command: &Command) {
        self.emit(answer(command, Ok(())));
    }

    fn finish(&self, command: &Command) {
        if self.answers().contains_key(command.request_id()) {
            self.result(command);
        }
    }

    fn synchronize_account(&mut self) -> Result<()> {
        let user = self.network.identity().map(|identity| identity.user_id);
        let generation = self.network.generation();
        if user == self.scope.user && generation == self.scope.network_generation {
            return Ok(());
        }
        self.scope = Scope {
            user,
            network_generation: generation,
            epoch: self.scope.epoch.checked_add(1).ok_or(Error::Stopped)?,
        };
        for connection in self.connections.values() {
            connection.disconnect();
        }
        self.connections.clear();
        for opening in std::mem::take(&mut self.opening).into_values() {
            opening.cancellation.cancel();
            for command in opening.commands {
                self.error(&command, &Error::Stale);
            }
        }
        self.views.clear();
        self.reconnect.clear();
        self.listed.clear();
        self.remotes.clear();
        self.starting.clear();
        self.cached_events.clear();
        self.room_latest.clear();
        self.room_seen = None;
        self.latest_auth = None;
        self.administering.clear();
        #[cfg(feature = "host")]
        self.publishing.clear();
        #[cfg(feature = "host")]
        self.retiring.clear();
        self.unpublishing.clear();
        #[cfg(feature = "host")]
        for local in self.local.values_mut() {
            local.published = false;
            local.entry.shared_with.clear();
            local.entry.mission_id = None;
            local.entry.mission_name = None;
            local.entry.owner_user_id.clone_from(&self.scope.user);
            local.entry.host_device_id = None;
        }
        #[cfg(feature = "host")]
        self.apply_wanted();
        self.emit(self.auth_event());
        self.publish_catalog();
        #[cfg(feature = "host")]
        self.dosi_account();
        Ok(())
    }

    fn network_event(&mut self, event: BackendEvent) {
        #[cfg(feature = "host")]
        self.dosi_account();
        if event.event.get("requestId").is_some() {
            let ends_sign_in = matches!(
                event.event.get("type").and_then(Value::as_str),
                Some("auth.result" | "auth.error")
            ) && self.latest_auth.take().is_some();
            self.emit(event.event);
            if ends_sign_in {
                self.emit(self.auth_event());
            }
            return;
        }
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
            .unwrap_or_default()
            .to_owned();
        let kind = kind.as_str();
        if kind == "network.sharing" {
            #[cfg(feature = "host")]
            self.sharing_changed(&event.event);
        } else if kind == "network.dosi" {
            #[cfg(feature = "host")]
            self.dosi_nudge(&event.event);
        } else if kind == "network.terminal" {
            self.terminal_changed(&event.event);
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
                Err(error) => tracing::warn!(%error, "remote session catalog was unreadable"),
            }
        } else {
            match kind {
                "auth.finalizing" | "auth.restoring" => {
                    self.latest_auth = Some(event.event.clone());
                }
                "auth.ready" | "auth.required" | "auth.error" => self.latest_auth = None,
                _ => {}
            }
            let event = if kind == "missions.snapshot" {
                self.with_unread(event.event)
            } else {
                event.event
            };
            if matches!(
                kind,
                "friends.snapshot"
                    | "devices.list"
                    | "devices.hosts"
                    | "devices.link.snapshot"
                    | "missions.snapshot"
                    | "system.updateRequired"
            ) {
                self.cached_events.insert(kind.to_owned(), event.clone());
            }
            let room = (kind == "room.snapshot").then(|| event["room"].clone());
            self.emit(event);
            if let Some(room) = room {
                self.room_changed(&room);
            }
        }
    }

    fn seen_rooms(&mut self) -> Option<&mut BTreeMap<String, u64>> {
        let user = self.scope.user.clone()?;
        if self
            .room_seen
            .as_ref()
            .is_none_or(|(owner, _)| *owner != user)
        {
            let seen = self.network.rooms_seen(&user);
            self.room_seen = Some((user, seen));
        }
        self.room_seen.as_mut().map(|(_, seen)| seen)
    }

    fn remember_seen(&self) {
        if let Some((user, seen)) = &self.room_seen
            && let Err(error) = self.network.remember_seen(user, seen)
        {
            tracing::warn!(%error, "could not save which room messages were seen");
        }
    }

    fn with_unread(&mut self, mut snapshot: Value) -> Value {
        let latest = self.room_latest.clone();
        let seen = self.seen_rooms().cloned().unwrap_or_default();
        for mission in snapshot
            .get_mut("missions")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let id = mission["id"].as_str().unwrap_or_default();
            let unread = latest
                .get(id)
                .is_some_and(|latest| *latest > seen.get(id).copied().unwrap_or_default());
            mission["unread"] = json!(unread);
        }
        snapshot
    }

    fn publish_rooms(&mut self) {
        if let Some(snapshot) = self.cached_events.get("missions.snapshot").cloned() {
            let snapshot = self.with_unread(snapshot);
            self.cached_events
                .insert("missions.snapshot".to_owned(), snapshot.clone());
            self.emit(snapshot);
        }
    }

    fn room_changed(&mut self, room: &Value) {
        let Some(id) = room["roomId"].as_str().map(str::to_owned) else {
            return;
        };
        let user = self.scope.user.clone();
        let Some(latest) = room["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|message| {
                message["agent"].is_string() || message["authorId"].as_str() != user.as_deref()
            })
            .filter_map(|message| message["sequence"].as_u64())
            .max()
        else {
            return;
        };
        let known = self.room_latest.get(&id).copied().unwrap_or_default();
        if latest <= known {
            return;
        }
        self.room_latest.insert(id.clone(), latest);
        if let Some(seen) = self.seen_rooms()
            && !seen.contains_key(&id)
        {
            seen.insert(id, latest);
            self.remember_seen();
        }
        self.publish_rooms();
    }

    fn see_room(&mut self, room: &str, sequence: u64) -> Result<()> {
        let seen = self
            .seen_rooms()
            .ok_or_else(|| Error::Invalid("Sign in to use rooms.".to_owned()))?;
        if seen.get(room).is_some_and(|seen| *seen >= sequence) {
            return Ok(());
        }
        seen.insert(room.to_owned(), sequence);
        self.remember_seen();
        self.publish_rooms();
        Ok(())
    }

    #[cfg(feature = "host")]
    fn sharing_changed(&mut self, event: &Value) {
        let id = event
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok());
        let incarnation = event
            .get("incarnationId")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok());
        let users = event
            .get("sharedWith")
            .cloned()
            .and_then(|users| serde_json::from_value::<Vec<String>>(users).ok());
        let room = event
            .get("missionId")
            .cloned()
            .and_then(|room| serde_json::from_value::<Option<Uuid>>(room).ok());
        let sequence = event.get("roomSequence").and_then(Value::as_u64);
        let administering = id.is_some_and(|id| self.administering.contains(&id));
        if let (Some(id), Some(incarnation), Some(users), Some(room), Some(sequence)) =
            (id, incarnation, users, room, sequence)
            && let Some(local) = self
                .local
                .get_mut(&id)
                .filter(|local| local.terminal.incarnation_id == incarnation)
        {
            local.entry.shared_with = users;
            if !administering {
                local.entry.mission_id = room.map(|room| room.to_string());
            }
            local.room_sequence = sequence;
            self.publish_catalog();
        }
    }

    fn network_reply(&mut self, reply: BackendReply) {
        for event in reply.events {
            self.network_event(BackendEvent {
                generation: reply.generation,
                user_id: reply.user_id.clone(),
                event,
            });
        }
    }

    fn replace_remotes(&mut self, remotes: Vec<RemoteSession>) {
        let remotes = self.end_orphaned_publications(remotes);
        #[cfg(feature = "host")]
        for remote in &remotes {
            if let Some(local) = self
                .local
                .get_mut(&remote.id)
                .filter(|local| local.terminal.incarnation_id == remote.incarnation_id)
                && !self.administering.contains(&remote.id)
                && !self.publishing.contains(&remote.id)
            {
                local.entry.name.clone_from(&remote.name);
                local.entry.mission_name = remote.mission_name.clone().filter(|_| {
                    local.entry.mission_id == remote.mission_id.map(|id| id.to_string())
                });
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
        let returned = remotes
            .iter()
            .filter(|remote| remote.online && !self.host_online(remote.id))
            .map(|remote| remote.id)
            .collect::<Vec<_>>();
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
        self.views
            .retain(|id, _| desired.contains_key(id) || self.opening.contains_key(id));
        self.connections.retain(|id, connection| {
            let keep =
                desired.get(id) == Some(&connection.incarnation_id) && !connection.is_closed();
            if !keep {
                connection.disconnect();
            }
            keep
        });
        let remotes = remotes
            .into_iter()
            .filter(|entry| !self.hosts(&entry.id))
            .collect::<Vec<_>>();
        self.listed = remotes
            .iter()
            .map(|entry| (entry.id, entry.clone()))
            .collect();
        self.remotes = remotes
            .into_iter()
            .map(|entry| (entry.id, self.remote_entry(entry)))
            .collect();
        for id in returned {
            self.reconnect_now(id);
        }
        self.publish_catalog();
        self.started_remotes();
    }

    fn started_remotes(&mut self) {
        let now = tokio::time::Instant::now();
        let shown = self
            .starting
            .iter()
            .filter(|(id, starting)| {
                starting.until <= now || self.listed.get(id).is_some_and(|listed| listed.online)
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in shown {
            if let Some(starting) = self.starting.remove(&id) {
                self.emit(starting.answer);
            }
        }
    }

    fn terminal_changed(&mut self, event: &Value) {
        let Ok(notice) = serde_json::from_value::<TerminalNotice>(event.clone()) else {
            tracing::warn!("a terminal notice was unreadable");
            return;
        };
        let id = notice.session_id;
        let Some(listed) = self
            .listed
            .get_mut(&id)
            .filter(|listed| listed.incarnation_id == notice.incarnation_id)
        else {
            return;
        };
        let returned = notice.online && !listed.online;
        listed.online = notice.online;
        listed.status = notice.status;
        listed.hidden &= !notice.online;
        let hidden = listed.hidden;
        self.started_remotes();
        let status = self.remote_status(id, notice.online);
        let Some(entry) = self
            .remotes
            .get_mut(&id)
            .filter(|entry| entry.incarnation_id == notice.incarnation_id.to_string())
        else {
            return;
        };
        entry.status = status;
        entry.hidden = hidden;
        if !notice.online {
            entry.program_status = None;
        }
        if self
            .connections
            .get(&id)
            .is_none_or(RemoteTerminal::is_closed)
        {
            self.show_shared_status(id);
        }
        if returned {
            self.reconnect_now(id);
        }
        self.publish_catalog();
    }

    fn host_online(&self, id: Uuid) -> bool {
        self.listed.get(&id).is_none_or(|remote| remote.online)
    }

    fn remote_status(&self, id: Uuid, online: bool) -> SessionStatus {
        if !online {
            SessionStatus::Offline
        } else if self.recovering(id) {
            SessionStatus::Reconnecting
        } else {
            SessionStatus::Running
        }
    }

    fn reconnect_now(&mut self, id: Uuid) {
        if let Some(retry) = self.reconnect.get_mut(&id) {
            retry.failures = 0;
            retry.next = tokio::time::Instant::now();
        }
    }

    fn listed_remote(&self, remote: &RemoteSession) -> RemoteSession {
        self.listed
            .get(&remote.id)
            .filter(|listed| listed.incarnation_id == remote.incarnation_id)
            .cloned()
            .unwrap_or_else(|| remote.clone())
    }

    fn show_shared_status(&mut self, id: Uuid) {
        let shared = self
            .listed
            .get(&id)
            .filter(|listed| listed.online)
            .map(|listed| (listed.incarnation_id.to_string(), listed.status.clone()));
        if let Some(entry) = self.remotes.get_mut(&id) {
            let status = shared
                .filter(|(incarnation, _)| *incarnation == entry.incarnation_id)
                .and_then(|(_, status)| status);
            entry.program = status.as_ref().and_then(|status| status.program.clone());
            entry.program_status = status.as_ref().and_then(TerminalStatus::program_status);
        }
    }

    fn end_orphaned_publications(&mut self, remotes: Vec<RemoteSession>) -> Vec<RemoteSession> {
        let Some(identity) = self.network.identity() else {
            return remotes;
        };
        let (orphaned, remotes): (Vec<_>, Vec<_>) = remotes.into_iter().partition(|remote| {
            remote.host_device_id == identity.device_id && !self.hosts(&remote.id)
        });
        for remote in orphaned {
            if remote.owner_user_id != identity.user_id
                || self.unpublishing.contains(&remote.id)
                || self.job_capacity().is_err()
            {
                continue;
            }
            self.unpublishing.insert(remote.id);
            let network = self.network.clone();
            let generation = self.scope.network_generation;
            self.spawn(async move {
                let result = if network.generation() == generation {
                    network
                        .end_hosted(remote.id, remote.incarnation_id)
                        .await
                        .map_err(Error::from)
                } else {
                    Err(Error::Stale)
                };
                Completion::Unpublished {
                    id: remote.id,
                    result,
                }
            });
        }
        remotes
    }

    pub(crate) fn recovering(&self, id: Uuid) -> bool {
        self.views.contains_key(&id)
            && self
                .connections
                .get(&id)
                .is_some_and(|connection| connection.is_interrupted() && !connection.is_closed())
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
        let live = connection_state == ConnectionState::Connected;
        let shared = remote.status.as_ref().filter(|_| remote.online);
        SessionEntry {
            attention: false,
            connected_users: if live {
                previous.map_or_else(Vec::new, |entry| entry.connected_users.clone())
            } else {
                vec![]
            },
            program: if live {
                previous.and_then(|entry| entry.program.clone())
            } else {
                shared.and_then(|status| status.program.clone())
            },
            prompt: false,
            repository: false,
            title: previous.and_then(|entry| entry.title.clone()),
            program_status: if live {
                previous
                    .filter(|_| remote.online)
                    .and_then(|entry| entry.program_status.clone())
            } else {
                shared.and_then(TerminalStatus::program_status)
            },
            context: if live {
                previous.and_then(|entry| entry.context.clone())
            } else {
                None
            },
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
            status: self.remote_status(remote.id, remote.online),
            connection_state,
            hidden: remote.hidden && !remote.online,
            message: self
                .remotes
                .get(&remote.id)
                .and_then(|entry| entry.message.clone()),
            mission_id: remote.mission_id.map(|id| id.to_string()),
            mission_name: remote.mission_name,
            shared_with: remote.shared_with,
            restored_from: None,
        }
    }

    #[cfg(feature = "host")]
    fn ended(&mut self, id: Uuid) {
        let (published, worktree) = self
            .local
            .remove(&id)
            .map_or((false, None), |local| (local.published, local.worktree));
        if published || self.publishing.contains(&id) {
            self.retiring.insert(id);
        }
        if let Some(worktree) = worktree {
            let inside = |directory: &Option<String>| {
                directory
                    .as_deref()
                    .is_some_and(|directory| Path::new(directory).starts_with(&worktree.path))
            };
            let starting = self.creating.values().any(|command| {
                matches!(command, Command::CreateSession { working_dir, .. } if inside(working_dir))
            });
            if !starting
                && !self
                    .local
                    .values()
                    .any(|other| inside(&other.entry.working_dir))
            {
                tokio::spawn(crate::worktree::remove_unchanged(worktree));
            }
        }
        self.publish_catalog();
        self.retire_publications();
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one match routes every terminal change"
    )]
    fn change(&mut self, change: SessionChange) {
        match change {
            SessionChange::RemoteEnded { id, instance_id } => {
                if self
                    .connections
                    .get(&id)
                    .is_some_and(|connection| connection.instance_id == instance_id)
                {
                    self.connections.remove(&id);
                    self.views.remove(&id);
                    self.reconnect.remove(&id);
                    self.remotes.remove(&id);
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Presence { id, users } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.connected_users = users;
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Cwd {
                id,
                path,
                repository,
            } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.repository = repository;
                    local.entry.working_dir = Some(path.to_string_lossy().into_owned());
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Program { id, program } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.program = program;
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Prompt { id, prompt } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.prompt = prompt;
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Title { id, title } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.title = Some(title.clone());
                    self.publish_catalog();
                }
                self.emit(json!({"type":"term.title", "sessionId":id, "title":title}));
            }
            #[cfg(feature = "host")]
            SessionChange::Bell { id } => {
                self.emit(json!({"type":"term.bell", "sessionId":id}));
                let calls = self
                    .local
                    .get(&id)
                    .is_some_and(|local| Activity::calls(&local.entry));
                let id = id.to_string();
                if calls && !self.attention.contains(&id) {
                    self.notice(&id);
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Status { id, status } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.program_status = status;
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Context { id, context } => {
                if let Some(local) = self.local.get_mut(&id) {
                    local.entry.context = context;
                    self.publish_catalog();
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Notification { id, title, body } => {
                if let Some(local) = self.local.get(&id) {
                    self.emit(json!({"type":"term.notification", "sessionId":id,
                        "runtimeIncarnationId":local.terminal.incarnation_id, "title":title, "body":body}));
                }
            }
            #[cfg(feature = "host")]
            SessionChange::Ended { id, reason } => {
                self.ended(id);
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
                    entry.program_status = metadata.status;
                    entry.context = metadata.context;
                    self.publish_catalog();
                }
            }
            SessionChange::RemoteInterrupted {
                id,
                incarnation,
                instance_id,
                reason,
            } => self.remote_lost(id, incarnation, instance_id, reason, true),
            SessionChange::RemoteClosed {
                id,
                incarnation,
                instance_id,
                reason,
            } => self.remote_lost(id, incarnation, instance_id, reason, false),
        }
    }

    fn remote_lost(
        &mut self,
        id: Uuid,
        incarnation: Uuid,
        instance_id: Uuid,
        reason: String,
        interrupted: bool,
    ) {
        if !self.connections.get(&id).is_some_and(|connection| {
            connection.incarnation_id == incarnation && connection.instance_id == instance_id
        }) {
            return;
        }
        tracing::warn!(%id, %reason, interrupted, "remote terminal connection lost");
        if interrupted && self.views.contains_key(&id) {
            let previous = self.reconnect.get(&id);
            let stable = previous
                .and_then(|retry| retry.connected)
                .is_some_and(|at| at.elapsed() >= crate::network::STABLE_CONNECTION);
            let failures = if stable {
                0
            } else {
                previous.map_or(0, |retry| retry.failures.saturating_add(1))
            };
            self.reconnect.insert(
                id,
                Reconnect {
                    incarnation,
                    failures,
                    next: tokio::time::Instant::now() + Reconnect::delay(failures),
                    connected: None,
                    failure: crate::diagnostics::Repeats::new(),
                },
            );
        } else if let Some(connection) = self.connections.remove(&id) {
            connection.disconnect();
        }
        let recovering = self.recovering(id);
        let online = self.host_online(id);
        let status = self.remote_status(id, online);
        if let Some(entry) = self.remotes.get_mut(&id) {
            entry.connected_users.clear();
            if recovering {
                entry.status = status;
                entry.message = reason
                    .ends_with(INPUT_NOT_SENT)
                    .then(|| INPUT_NOT_SENT.to_owned());
            } else {
                entry.connection_state = ConnectionState::Offline;
                entry.message = online.then_some(reason);
            }
        }
        if !recovering {
            self.show_shared_status(id);
        }
        self.publish_catalog();
    }

    fn reconnect_views(&mut self) {
        let connects = self.network.inner.link.connects();
        if connects != self.link_connects {
            self.link_connects = connects;
            let recovering = self
                .reconnect
                .keys()
                .copied()
                .filter(|id| self.recovering(*id))
                .collect::<Vec<_>>();
            for id in recovering {
                self.reconnect_now(id);
            }
        }
        let now = tokio::time::Instant::now();
        let ids = self
            .reconnect
            .iter()
            .filter(|(id, retry)| {
                retry.next <= now
                    && self.host_online(**id)
                    && self
                        .connections
                        .get(id)
                        .is_none_or(RemoteTerminal::is_interrupted)
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

    #[cfg(feature = "host")]
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
                    network.publish(info, host_requests, output).await
                } else {
                    Err(crate::network::Error::Stale)
                };
                Completion::Published {
                    id,
                    incarnation,
                    refused: matches!(
                        result,
                        Err(crate::network::Error::Backend {
                            status: 403 | 404,
                            ..
                        })
                    ),
                    result: result.map_err(Error::from),
                }
            });
        }
    }

    #[cfg(feature = "host")]
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
