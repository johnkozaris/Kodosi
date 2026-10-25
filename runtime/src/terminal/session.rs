use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use kodosi_pty::{KodosiPty, PtyReader, ShutdownStage, Transfer, WaitOutcome};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::context::{Contexts, LocalMachine, TerminalContext};
use super::emulator::{SessionTerminalHandle, TerminalEffect};
use super::status::ProgramStatusRecords;
use super::subscribers::Subscribers;
use super::title::titles_look_same;
use super::{
    AGENT_ASKS, CellPixels, ControlFrame, DataFrame, NO_AGENT, ProgramState, SOMEONE_TYPING,
    SessionChange, Subscription, TYPING, TerminalPixelGeometry, TerminalSize,
};
use crate::{
    Error, Result,
    network::{CheckpointCut, HostRequest, PublishedFrame, TerminalControl},
};

const COMMAND_CAPACITY: usize = 64;
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
const READ_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const START_PATIENCE: Duration = Duration::from_secs(3);
const START_LIMIT: Duration = Duration::from_secs(15);
const PROMPT_QUIET: Duration = Duration::from_millis(300);
const BUSY_UPKEEP: Duration = Duration::from_millis(20);
const IDLE_UPKEEP: Duration = Duration::from_millis(250);
const BUSY_AFTER_OUTPUT: Duration = Duration::from_secs(1);
const SIZE_NOT_APPLIED: &str = "The terminal could not apply its new size.";

type Completion = Option<std::result::Result<(), String>>;

enum Request {
    Subscribe(oneshot::Sender<Result<Subscription>>),
    Checkpoint {
        connection: Uuid,
        reply: oneshot::Sender<Result<CheckpointCut>>,
    },
    Unsubscribe(Uuid),
    Input {
        connection: Uuid,
        write: PendingWrite,
    },
    /// Input from a command; `true` makes it a message, which only an agent that takes messages
    /// gets.
    Send(PendingWrite, bool),
    Read {
        history: bool,
        reply: oneshot::Sender<Result<String>>,
    },
    Resize {
        connection: Uuid,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
        reply: oneshot::Sender<Result<()>>,
    },
    Focus {
        connection: Uuid,
        focused: bool,
        reply: oneshot::Sender<Result<()>>,
    },
    Interrupt(oneshot::Sender<Result<()>>),
    Run {
        command: String,
        reply: oneshot::Sender<Result<()>>,
    },
    Start(String),
    Foreground {
        variable: &'static str,
        reply: oneshot::Sender<Result<Foreground>>,
    },
}

/// The process group in front of a terminal's shell, and one variable of its leader.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Foreground {
    pub group: Option<u32>,
    pub variable: Option<String>,
}

#[derive(Clone)]
pub(crate) struct TerminalSession {
    pub incarnation_id: Uuid,
    commands: mpsc::Sender<Request>,
    pub host_requests: mpsc::Sender<HostRequest>,
    output: broadcast::Sender<PublishedFrame>,
    cancellation: CancellationToken,
    completion: watch::Receiver<Completion>,
    input_budget: Arc<Semaphore>,
}

impl TerminalSession {
    /// Starts a terminal. `notes` is the folder with Kodosi's note for agents, which Copilot CLI
    /// reads; the note itself is in each terminal's environment.
    pub(crate) async fn spawn(
        id: Uuid,
        incarnation_id: Uuid,
        program: PathBuf,
        arguments: Vec<String>,
        directory: PathBuf,
        prompt_marks: Option<PathBuf>,
        notes: Option<PathBuf>,
        changes: mpsc::Sender<SessionChange>,
    ) -> Result<Self> {
        let size = TerminalSize::default();
        let clear_line = clear_line_keys(&program);
        let launch_directory = directory.clone();
        let (emulator, pty, reader) = tokio::task::spawn_blocking(move || -> Result<_> {
            let cwd = directory
                .to_str()
                .ok_or_else(|| Error::Invalid("working directory must be UTF-8".to_owned()))?;
            let emulator = SessionTerminalHandle::spawn(size)?;
            let session_id = id.to_string();
            let command_path = command_path();
            let copilot_notes = notes.as_deref().and_then(copilot_notes);
            let mut environment = vec![
                ("KODOSI_SESSION_ID", session_id.as_str()),
                ("KODOSI_VERSION", env!("KODOSI_VERSION")),
                (
                    crate::agents::CONTEXT_VARIABLE,
                    crate::agents::CONTEXT.trim(),
                ),
            ];
            if let Some(path) = &command_path {
                environment.push(("PATH", path.as_str()));
            }
            if let Some(folders) = &copilot_notes {
                environment.push((crate::agents::COPILOT_INSTRUCTIONS, folders.as_str()));
            }
            let (pty, reader) = KodosiPty::spawn_program(
                &program,
                &arguments,
                Some(cwd),
                size.rows(),
                size.cols(),
                &environment,
                prompt_marks.as_deref(),
            )?;
            Ok((emulator, pty, reader))
        })
        .await
        .map_err(|error| Error::Other(format!("Terminal startup failed: {error}")))??;
        let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (host_requests, host_receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (output, _) = broadcast::channel(256);
        let cancellation = CancellationToken::new();
        let (completed, completion) = watch::channel(None);
        let input_budget = Arc::new(Semaphore::new(MAX_PENDING_BYTES));
        let session = Self {
            incarnation_id,
            commands,
            host_requests,
            output: output.clone(),
            cancellation: cancellation.clone(),
            completion,
            input_budget: Arc::clone(&input_budget),
        };
        let actor = LocalActor {
            id,
            incarnation: incarnation_id,
            pty,
            reader,
            emulator,
            commands: receiver,
            host: host_receiver,
            output,
            changes,
            cancellation,
            completed,
            input_budget,
            subscribers: Subscribers::default(),
            queue: VecDeque::new(),
            focused: HashMap::new(),
            resize_owner: None,
            requested: HashMap::new(),
            typed_at: None,
            cell: CellPixels::default(),
            sequence: 0,
            host_stop_reply: None,
            program: None,
            clear_line,
            start: None,
            output_at: None,
            prompt: false,
            viewers: HashMap::new(),
            owner: None,
            local_view: false,
            working_directory: launch_directory,
            observed_directory: None,
            title: None,
            published_title: None,
            status: ProgramStatusRecords::default(),
            status_program: None,
            published_status: None,
            contexts: Contexts::default(),
            published_context: None,
            metadata_dirty: false,
            next_program_check: tokio::time::Instant::now() + Duration::from_secs(1),
            next_refresh: tokio::time::Instant::now(),
        };
        tokio::spawn(actor.run());
        Ok(session)
    }

    pub(crate) fn output(&self) -> broadcast::Receiver<PublishedFrame> {
        self.output.subscribe()
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.cancellation.is_cancelled() || self.commands.is_closed()
    }

    pub(crate) fn subscribe(&self) -> impl Future<Output = Result<Subscription>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(self.send(Request::Subscribe(reply)), response)
    }

    pub(crate) fn checkpoint(
        &self,
        connection: Uuid,
    ) -> impl Future<Output = Result<CheckpointCut>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(
            self.send(Request::Checkpoint { connection, reply }),
            response,
        )
    }

    pub(crate) fn unsubscribe(&self, connection: Uuid) {
        drop(self.send(Request::Unsubscribe(connection)));
    }

    #[cfg(all(test, unix))]
    pub(crate) fn input(&self, connection: Uuid, bytes: Bytes) -> Result<()> {
        let write = PendingWrite::new(bytes, &self.input_budget, None)?;
        self.send(Request::Input { connection, write })
    }

    pub(crate) fn write_input(
        &self,
        connection: Uuid,
        bytes: Bytes,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        let admission = PendingWrite::new(bytes, &self.input_budget, None).and_then(|mut write| {
            write.completion = Some(InputCompletion::Local(reply));
            self.send(Request::Input { connection, write })
        });
        await_reply(admission, response)
    }

    pub(crate) fn send_input(
        &self,
        bytes: Bytes,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        let admission = PendingWrite::new(bytes, &self.input_budget, None).and_then(|mut write| {
            write.completion = Some(InputCompletion::Local(reply));
            self.send(Request::Send(write, false))
        });
        await_reply(admission, response)
    }

    /// Types a message for the agent in this terminal, unless it waits for a person's answer
    /// or a person types here.
    pub(crate) fn message(&self, bytes: Bytes) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        let admission = PendingWrite::new(bytes, &self.input_budget, None).and_then(|mut write| {
            write.completion = Some(InputCompletion::Local(reply));
            self.send(Request::Send(write, true))
        });
        await_reply(admission, response)
    }

    pub(crate) fn read(
        &self,
        history: bool,
    ) -> impl Future<Output = Result<String>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(self.send(Request::Read { history, reply }), response)
    }

    pub(crate) fn resize(
        &self,
        connection: Uuid,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        let admission = validate_pixels(size, geometry).and_then(|()| {
            self.send(Request::Resize {
                connection,
                size,
                geometry,
                claim,
                reply,
            })
        });
        await_reply(admission, response)
    }

    pub(crate) fn focus(
        &self,
        connection: Uuid,
        focused: bool,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(
            self.send(Request::Focus {
                connection,
                focused,
                reply,
            }),
            response,
        )
    }

    pub(crate) fn interrupt(&self) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(self.send(Request::Interrupt(reply)), response)
    }

    pub(crate) fn run(&self, command: String) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(self.send(Request::Run { command, reply }), response)
    }

    pub(crate) fn start(&self, command: String) -> Result<()> {
        self.send(Request::Start(command))
    }

    pub(crate) fn foreground(
        &self,
        variable: &'static str,
    ) -> impl Future<Output = Result<Foreground>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        await_reply(self.send(Request::Foreground { variable, reply }), response)
    }

    pub(crate) fn close(&self) -> impl Future<Output = Result<()>> + Send + use<> {
        self.cancellation.cancel();
        let mut completion = self.completion.clone();
        async move {
            tokio::time::timeout(STOP_TIMEOUT, async {
                loop {
                    let result = completion.borrow_and_update().clone();
                    if let Some(result) = result {
                        return result.map_err(Error::Other);
                    }
                    completion.changed().await.map_err(|_| {
                        Error::Other("Terminal shutdown was not confirmed.".to_owned())
                    })?;
                }
            })
            .await
            .map_err(|_| Error::Other("Terminal shutdown was not confirmed in time.".to_owned()))?
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }

    fn send(&self, command: Request) -> Result<()> {
        if self.is_closed() {
            return Err(Error::Stopped);
        }
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Stopped,
            })
    }
}

async fn await_reply<T>(
    admission: Result<()>,
    response: oneshot::Receiver<Result<T>>,
) -> Result<T> {
    admission?;
    tokio::time::timeout(REQUEST_TIMEOUT, response)
        .await
        .map_err(|_| Error::Other("terminal request timed out".to_owned()))?
        .map_err(|_| Error::Stopped)?
}

enum InputCompletion {
    Local(oneshot::Sender<Result<()>>),
    Remote(oneshot::Sender<std::result::Result<serde_json::Value, String>>),
}

impl InputCompletion {
    fn send(self, result: Result<()>) {
        match self {
            Self::Local(reply) => drop(reply.send(result)),
            Self::Remote(reply) => drop(
                reply.send(
                    result
                        .map(|()| serde_json::json!({"accepted": true}))
                        .map_err(|error| error.to_string()),
                ),
            ),
        }
    }
}

struct PendingWrite {
    bytes: Bytes,
    offset: usize,
    authorization: Option<CancellationToken>,
    completion: Option<InputCompletion>,
    _budget: OwnedSemaphorePermit,
}

impl Drop for PendingWrite {
    fn drop(&mut self) {
        if let Some(completion) = self.completion.take() {
            completion.send(Err(Error::Other(
                "Input delivery was interrupted; do not retry automatically.".to_owned(),
            )));
        }
    }
}

impl PendingWrite {
    fn new(
        bytes: Bytes,
        budget: &Arc<Semaphore>,
        authorization: Option<CancellationToken>,
    ) -> Result<Self> {
        if bytes.len() > super::TERMINAL_INPUT_MAX_BYTES {
            return Err(Error::Invalid("terminal input exceeds 1 MiB".to_owned()));
        }
        let count = u32::try_from(bytes.len()).map_err(|_| Error::Busy)?;
        let permit = Arc::clone(budget)
            .try_acquire_many_owned(count)
            .map_err(|_| Error::Busy)?;
        Ok(Self {
            bytes,
            offset: 0,
            authorization,
            completion: None,
            _budget: permit,
        })
    }

    fn is_cancelled(&self) -> bool {
        self.authorization
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    }

    fn complete(&mut self) {
        if let Some(completion) = self.completion.take() {
            completion.send(Ok(()));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Controller {
    Local(Uuid),
    Remote(Uuid),
}

struct ResizeOwner {
    controller: Controller,
    authorization: CancellationToken,
}

/// The last size that a view asked for.
#[derive(Clone, Copy)]
struct Requested {
    size: TerminalSize,
    geometry: Option<TerminalPixelGeometry>,
    /// Higher for a view whose size the terminal took later, and zero before it takes one.
    applied: u64,
}

enum ResizeError {
    Rejected(Error),
    Diverged(Error),
}

impl ResizeError {
    fn split(self) -> (Error, bool) {
        match self {
            Self::Rejected(error) => (error, false),
            Self::Diverged(error) => (error, true),
        }
    }
}

fn validate_pixels(size: TerminalSize, geometry: Option<TerminalPixelGeometry>) -> Result<()> {
    let Some(geometry) = geometry else {
        return Ok(());
    };
    let geometry = geometry
        .validate_for_size(size)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    u16::try_from(geometry.width_pixels())
        .map_err(|_| Error::Invalid("terminal pixel width exceeds the PTY limit".to_owned()))?;
    u16::try_from(geometry.height_pixels())
        .map_err(|_| Error::Invalid("terminal pixel height exceeds the PTY limit".to_owned()))?;
    Ok(())
}

fn command_path() -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let program = std::env::current_exe().ok()?;
    path_with_command(
        &std::env::var_os("PATH").unwrap_or_default(),
        program.parent()?,
    )
}

/// Kodosi's notes folder after the folders that Copilot CLI already reads.
fn copilot_notes(folder: &Path) -> Option<String> {
    let folder = folder.to_str()?;
    Some(
        std::env::var(crate::agents::COPILOT_INSTRUCTIONS)
            .ok()
            .filter(|folders| !folders.is_empty())
            .map_or_else(
                || folder.to_owned(),
                |folders| format!("{folders},{folder}"),
            ),
    )
}

fn path_with_command(path: &std::ffi::OsStr, folder: &Path) -> Option<String> {
    if !folder.join("kodosi").is_file() {
        return None;
    }
    let others = std::env::split_paths(path).filter(|other| other.as_path() != folder);
    std::env::join_paths(std::iter::once(folder.to_path_buf()).chain(others))
        .ok()?
        .into_string()
        .ok()
}

/// `PowerShell` and `cmd` cancel a line with Ctrl+C; Unix line editors clear it
/// with Ctrl+E, Ctrl+U.
fn clear_line_keys(shell: &Path) -> &'static str {
    let stem = shell
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if ["pwsh", "powershell", "cmd"]
        .iter()
        .any(|windows| windows.eq_ignore_ascii_case(stem))
    {
        "\x03"
    } else {
        "\x05\x15"
    }
}

struct LocalActor {
    id: Uuid,
    incarnation: Uuid,
    pty: KodosiPty,
    reader: PtyReader,
    emulator: SessionTerminalHandle,
    commands: mpsc::Receiver<Request>,
    host: mpsc::Receiver<HostRequest>,
    output: broadcast::Sender<PublishedFrame>,
    changes: mpsc::Sender<SessionChange>,
    cancellation: CancellationToken,
    completed: watch::Sender<Completion>,
    input_budget: Arc<Semaphore>,
    subscribers: Subscribers,
    queue: VecDeque<PendingWrite>,
    focused: HashMap<Controller, CancellationToken>,
    resize_owner: Option<ResizeOwner>,
    requested: HashMap<Controller, Requested>,
    typed_at: Option<tokio::time::Instant>,
    cell: CellPixels,
    sequence: u64,
    host_stop_reply: Option<oneshot::Sender<std::result::Result<serde_json::Value, String>>>,
    program: Option<String>,
    clear_line: &'static str,
    start: Option<(String, tokio::time::Instant)>,
    output_at: Option<tokio::time::Instant>,
    prompt: bool,
    viewers: HashMap<Uuid, String>,
    owner: Option<String>,
    local_view: bool,
    working_directory: PathBuf,
    observed_directory: Option<PathBuf>,
    title: Option<String>,
    published_title: Option<String>,
    status: ProgramStatusRecords,
    status_program: Option<u32>,
    published_status: Option<super::ProgramStatus>,
    contexts: Contexts,
    published_context: Option<TerminalContext>,
    metadata_dirty: bool,
    next_program_check: tokio::time::Instant,
    next_refresh: tokio::time::Instant,
}

impl LocalActor {
    fn connected_users(&self) -> Vec<String> {
        self.viewers
            .values()
            .chain(self.owner.iter().filter(|_| self.local_view))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(crate::protocol::TERMINAL_VIEWERS_MOST)
            .collect()
    }

    fn note_local_view(&mut self) {
        let showing = self.subscribers.has_active();
        if showing != self.local_view {
            self.local_view = showing;
            if self.owner.is_some() {
                self.publish_presence();
            }
        }
    }

    fn publish_presence(&mut self) {
        self.metadata_dirty = true;
        drop(self.changes.try_send(SessionChange::Presence {
            id: self.id,
            users: self.connected_users(),
        }));
    }

    fn metadata(&self) -> super::TerminalMetadata {
        super::TerminalMetadata {
            connected_users: self.connected_users(),
            directory: Some(self.working_directory.to_string_lossy().into_owned()),
            title: self
                .title
                .as_ref()
                .map(|title| title.chars().take(1024).collect()),
            program: self.live_program(),
            status: self.status.summary(),
            context: self.published_context.clone(),
        }
    }

    fn publish_context(&mut self) {
        let context = self.contexts.summary(LocalMachine::get());
        if context != self.published_context
            && self
                .changes
                .try_send(SessionChange::Context {
                    id: self.id,
                    context: context.clone(),
                })
                .is_ok()
        {
            self.published_context = context;
            self.metadata_dirty = true;
        }
    }

    fn publish_status(&mut self) {
        let status = self.status.summary();
        if status != self.published_status
            && self
                .changes
                .try_send(SessionChange::Status {
                    id: self.id,
                    status: status.clone(),
                })
                .is_ok()
        {
            self.published_status = status;
            self.metadata_dirty = true;
        }
    }

    fn metadata_checkpoint(&self, mut checkpoint: super::Checkpoint) -> super::Checkpoint {
        checkpoint.metadata = Some(Box::new(self.metadata()));
        checkpoint
    }

    fn enqueue(&mut self, bytes: Bytes, authorization: Option<CancellationToken>) -> Result<()> {
        self.queue
            .push_back(PendingWrite::new(bytes, &self.input_budget, authorization)?);
        Ok(())
    }

    fn drop_cancelled_input(&mut self) {
        let in_flight = self.reader.write_in_flight();
        let mut index = 0;
        self.queue.retain(|write| {
            let keep = (in_flight && index == 0) || !write.is_cancelled();
            index += 1;
            keep
        });
    }

    fn pending_input(&mut self) -> Option<Bytes> {
        let in_flight = self.reader.write_in_flight();
        while let Some(write) = self.queue.front_mut() {
            if in_flight {
                return Some(write.bytes.slice(write.offset..));
            }
            if write.is_cancelled() {
                self.queue.pop_front();
            } else if write.offset == write.bytes.len() {
                write.complete();
                self.queue.pop_front();
            } else {
                return Some(write.bytes.slice(write.offset..));
            }
        }
        None
    }

    fn wrote(&mut self, count: usize) {
        if let Some(write) = self.queue.front_mut() {
            write.offset = (write.offset + count).min(write.bytes.len());
            if write.offset == write.bytes.len() {
                write.complete();
                self.queue.pop_front();
            }
        }
    }

    async fn apply_resize(
        &mut self,
        controller: Controller,
        authorization: CancellationToken,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
    ) -> std::result::Result<(), ResizeError> {
        validate_pixels(size, geometry).map_err(ResizeError::Rejected)?;
        let applied = self
            .requested
            .get(&controller)
            .map_or(0, |requested| requested.applied);
        self.requested.insert(
            controller,
            Requested {
                size,
                geometry,
                applied,
            },
        );
        let claim = claim || self.focused.contains_key(&controller);
        let cell = geometry.map_or(self.cell, CellPixels::of);
        let (width, height) = cell.pty_pixels(size);
        if authorization.is_cancelled() {
            return Err(ResizeError::Rejected(Error::Stale));
        }
        if self.resize_owner.as_ref().is_some_and(|owner| {
            !owner.authorization.is_cancelled() && owner.controller != controller
        }) && !claim
        {
            return Err(ResizeError::Rejected(Error::Invalid(
                "Another terminal view controls the size. Focus this view to resize it.".to_owned(),
            )));
        }
        self.pty
            .resize(size.rows(), size.cols(), width, height)
            .map_err(Error::from)
            .map_err(ResizeError::Rejected)?;
        let effects = self
            .emulator
            .resize(size.rows(), size.cols(), cell)
            .await
            .map_err(ResizeError::Diverged)?;
        self.cell = cell;
        for bytes in effects {
            self.enqueue(Bytes::from(bytes), None)
                .map_err(ResizeError::Diverged)?;
        }
        let applied = self
            .requested
            .values()
            .map(|requested| requested.applied)
            .max()
            .map_or(1, |latest| latest + 1);
        self.requested.insert(
            controller,
            Requested {
                size,
                geometry,
                applied,
            },
        );
        self.resize_owner = Some(ResizeOwner {
            controller,
            authorization,
        });
        self.subscribers.control(&ControlFrame::Resize {
            rows: size.rows(),
            cols: size.cols(),
            cell,
            at_sequence: self.sequence,
        });
        drop(self.output.send(PublishedFrame::Resize {
            rows: size.rows(),
            cols: size.cols(),
            cell,
            at_sequence: self.sequence,
        }));
        Ok(())
    }

    async fn set_focus(
        &mut self,
        controller: Controller,
        authorization: Option<CancellationToken>,
        focused: bool,
    ) -> Result<()> {
        if authorization
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(Error::Stale);
        }
        let was_focused = !self.focused.is_empty();
        if let Some(authorization) = &authorization {
            if focused {
                self.focused.insert(controller, authorization.clone());
            } else {
                self.focused.remove(&controller);
            }
        } else {
            self.focused.remove(&controller);
        }
        let is_focused = !self.focused.is_empty();
        if was_focused != is_focused {
            let bytes = self.emulator.set_focused(is_focused).await?;
            self.enqueue(
                Bytes::from(bytes),
                if focused { authorization } else { None },
            )?;
        }
        Ok(())
    }

    async fn focus_view(
        &mut self,
        controller: Controller,
        authorization: CancellationToken,
        focused: bool,
    ) -> (Result<()>, bool) {
        if let Err(error) = self
            .set_focus(controller, Some(authorization.clone()), focused)
            .await
        {
            return (Err(error), false);
        }
        let owner = self
            .resize_owner
            .as_ref()
            .is_some_and(|owner| owner.controller == controller);
        if !focused || owner {
            return (Ok(()), false);
        }
        match self.take_requested_size(controller, authorization).await {
            Ok(()) => (Ok(()), false),
            Err(error) => (Err(error), true),
        }
    }

    /// Applies the size that a focused view asked for. An error means that the terminal and its
    /// process can disagree about the size.
    async fn take_requested_size(
        &mut self,
        controller: Controller,
        authorization: CancellationToken,
    ) -> Result<()> {
        let Some(requested) = self.requested.get(&controller).copied() else {
            return Ok(());
        };
        match self
            .apply_resize(
                controller,
                authorization,
                requested.size,
                requested.geometry,
                true,
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(ResizeError::Rejected(error)) => {
                tracing::debug!(id = %self.id, %error, "a focused view kept the terminal size");
                Ok(())
            }
            Err(ResizeError::Diverged(error)) => Err(error),
        }
    }

    /// The view that set the size left, so the focused view whose size the terminal took last
    /// takes the size back. The answer is a reason to stop the terminal.
    async fn give_size_back(&mut self) -> Option<String> {
        self.resize_owner = None;
        let (controller, authorization) = self
            .focused
            .iter()
            .filter(|(_, authorization)| !authorization.is_cancelled())
            .filter_map(|(controller, authorization)| {
                self.requested
                    .get(controller)
                    .map(|requested| (requested.applied, *controller, authorization))
            })
            .max_by_key(|(applied, ..)| *applied)
            .map(|(_, controller, authorization)| (controller, authorization.clone()))?;
        let error = self
            .take_requested_size(controller, authorization)
            .await
            .err()?;
        tracing::warn!(id = %self.id, %error, "the terminal could not give the size back");
        Some(SIZE_NOT_APPLIED.to_owned())
    }

    fn typing(&self) -> bool {
        self.typed_at.is_some_and(|at| at.elapsed() < TYPING)
    }

    /// Ends a view's focus and its claim on the size. The answer is a reason to stop the
    /// terminal.
    async fn release(&mut self, controller: Controller) -> Option<String> {
        self.requested.remove(&controller);
        if self.focused.contains_key(&controller)
            && let Err(error) = self.set_focus(controller, None, false).await
        {
            tracing::warn!(id = %self.id, %error, "terminal view release was incomplete");
        }
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|owner| owner.controller == controller)
        {
            return self.give_size_back().await;
        }
        None
    }

    fn type_start_line(&mut self) {
        let Some((_, since)) = &self.start else {
            return;
        };
        let waited = since.elapsed();
        let quiet = self
            .output_at
            .is_some_and(|output| output.elapsed() >= PROMPT_QUIET);
        let prompt = self.pty.foreground_is_child()
            && (self.pty.reads_keys()
                || (kodosi_pty::CONSOLE_HOST && quiet)
                || waited >= START_PATIENCE);
        if !prompt && waited < START_LIMIT {
            return;
        }
        let Some((line, _)) = self.start.take() else {
            return;
        };
        if !prompt {
            tracing::info!(id = %self.id, "the start line was dropped because the shell runs another program");
        } else if let Err(error) = self.enqueue(Bytes::from(format!("{line}\r")), None) {
            tracing::warn!(id = %self.id, %error, "the start line was not typed");
        }
    }

    fn busy(&self) -> bool {
        self.start.is_some()
            || !self.queue.is_empty()
            || self.subscribers.behind()
            || self
                .output_at
                .is_some_and(|output| output.elapsed() < BUSY_AFTER_OUTPUT)
    }

    /// The answer is a reason to stop the terminal.
    async fn maintain(&mut self) -> Option<String> {
        self.subscribers.prune();
        self.type_start_line();
        if self.subscribers.caught_up() && tokio::time::Instant::now() >= self.next_refresh {
            match self.emulator.checkpoint_data(true, u64::MAX).await {
                Ok(cut) => self
                    .subscribers
                    .refresh(&cut.checkpoint, cut.applied_sequence),
                Err(error) => {
                    tracing::warn!(id = %self.id, %error, "terminal view refresh failed; it will be tried again");
                    self.next_refresh = tokio::time::Instant::now() + Duration::from_secs(1);
                }
            }
        }
        if tokio::time::Instant::now() >= self.next_program_check {
            self.next_program_check = tokio::time::Instant::now() + Duration::from_secs(1);
            if let Some(path) = self.pty.working_directory()
                && self.observed_directory.as_ref() != Some(&path)
                && self.set_working_directory(path.clone())
            {
                self.observed_directory = Some(path);
            }
            let program = self.live_program();
            let prompt = self.at_prompt(program.as_deref());
            if program != self.program
                && self
                    .changes
                    .try_send(SessionChange::Program {
                        id: self.id,
                        program: program.clone(),
                    })
                    .is_ok()
            {
                self.program = program;
                self.metadata_dirty = true;
            }
            if prompt != self.prompt
                && self
                    .changes
                    .try_send(SessionChange::Prompt {
                        id: self.id,
                        prompt,
                    })
                    .is_ok()
            {
                self.prompt = prompt;
            }
            if self.status_program.is_some()
                && self.pty.foreground_process_group() != self.status_program
            {
                self.status_program = None;
                self.status.program_exited();
            }
            if self.pty.foreground_is_child()
                && let Some(shell) = self.pty.foreground_process_group()
            {
                self.contexts.shell_in_front(shell);
            }
        }
        self.publish_status();
        self.publish_context();
        if !titles_look_same(self.title.as_deref(), self.published_title.as_deref())
            && let Some(title) = &self.title
            && self
                .changes
                .try_send(SessionChange::Title {
                    id: self.id,
                    title: title.clone(),
                })
                .is_ok()
        {
            self.published_title.clone_from(&self.title);
        }
        if self.metadata_dirty {
            self.metadata_dirty = false;
            drop(self.output.send(PublishedFrame::Metadata(self.metadata())));
        }
        let expired = self
            .focused
            .iter()
            .filter(|(_, token)| token.is_cancelled())
            .map(|(controller, _)| *controller)
            .collect::<Vec<_>>();
        for controller in expired {
            if let Some(reason) = self.release(controller).await {
                return Some(reason);
            }
        }
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|owner| owner.authorization.is_cancelled())
            && let Some(reason) = self.give_size_back().await
        {
            return Some(reason);
        }
        self.drop_cancelled_input();
        None
    }

    async fn apply_output(&mut self, bytes: Bytes, accepting_input: bool) -> Result<()> {
        self.output_at = Some(tokio::time::Instant::now());
        let applied = self.emulator.process_output(bytes.clone()).await?;
        if applied.processing_failed
            || applied.applied_sequence != self.sequence.checked_add(1).ok_or(Error::Stopped)?
        {
            return Err(Error::Other(
                "terminal parser failed to apply output".to_owned(),
            ));
        }
        self.subscribers.data(&DataFrame {
            sequence: self.sequence,
            bytes: bytes.clone(),
        });
        drop(self.output.send(PublishedFrame::Raw {
            sequence: self.sequence,
            bytes,
        }));
        self.sequence = applied.applied_sequence;
        for effect in applied.effects {
            match effect {
                TerminalEffect::PtyWrite(bytes) => {
                    if accepting_input && let Err(error) = self.enqueue(Bytes::from(bytes), None) {
                        tracing::warn!(id = %self.id, %error, "terminal reply dropped because the input queue is full");
                    }
                }
                TerminalEffect::Cwd(path) => {
                    self.set_working_directory(path);
                }
                TerminalEffect::Title(title) => {
                    self.metadata_dirty |= !titles_look_same(self.title.as_deref(), Some(&title));
                    self.title = Some(title);
                }
                TerminalEffect::Bell => {
                    drop(self.changes.try_send(SessionChange::Bell { id: self.id }));
                }
                TerminalEffect::ProgramStatus(report) => {
                    self.status.apply(report);
                    self.status_program = self.pty.foreground_process_group();
                    self.publish_status();
                }
                TerminalEffect::Progress(report) => {
                    self.status.apply_progress(report);
                    self.status_program = self.pty.foreground_process_group();
                    self.publish_status();
                }
                TerminalEffect::Reset => {
                    self.status.reset();
                    self.publish_status();
                }
                TerminalEffect::Context(signal) => {
                    self.contexts
                        .apply(signal, self.pty.foreground_process_group());
                    self.publish_context();
                }
                TerminalEffect::DesktopNotification { title, body } => {
                    drop(self.changes.try_send(SessionChange::Notification {
                        id: self.id,
                        title,
                        body,
                    }));
                }
            }
        }
        Ok(())
    }

    fn live_program(&self) -> Option<String> {
        self.status
            .live_program()
            .map(str::to_owned)
            .or_else(|| self.pty.foreground_program())
    }

    fn set_working_directory(&mut self, path: PathBuf) -> bool {
        if path == self.working_directory {
            return true;
        }
        if self
            .changes
            .try_send(SessionChange::Cwd {
                id: self.id,
                repository: crate::worktree::is_repository(&path),
                path: path.clone(),
            })
            .is_err()
        {
            return false;
        }
        self.working_directory = path;
        self.metadata_dirty = true;
        true
    }

    fn at_prompt(&self, program: Option<&str>) -> bool {
        program.is_none() && self.pty.foreground_is_child()
    }

    fn accept_input(&mut self, connection: Uuid, mut write: PendingWrite) {
        let Ok(authorization) = self.subscribers.authorization(connection) else {
            if let Some(completion) = write.completion.take() {
                completion.send(Err(Error::Stale));
            }
            return;
        };
        write.authorization = Some(authorization);
        self.typed_at = Some(tokio::time::Instant::now());
        self.queue.push_back(write);
    }

    fn accept_command_input(&mut self, mut write: PendingWrite, message: bool) {
        let refused = if message {
            self.takes_message().err()
        } else {
            self.typing()
                .then(|| Error::Invalid(SOMEONE_TYPING.to_owned()))
        };
        if let Some(error) = refused {
            if let Some(completion) = write.completion.take() {
                completion.send(Err(error));
            }
            return;
        }
        self.queue.push_back(write);
    }

    /// Typed text answers a question that waits, so a message goes only to an agent that reports
    /// its state and does not wait for a person's answer, and never while a person types.
    fn takes_message(&self) -> Result<()> {
        if self.typing() {
            return Err(Error::Invalid(SOMEONE_TYPING.to_owned()));
        }
        let agent = self
            .live_program()
            .is_some_and(|program| crate::agents::is_agent_name(&program));
        match self.status.summary() {
            Some(status) if agent && status.state == ProgramState::Blocked => {
                Err(Error::Invalid(AGENT_ASKS.to_owned()))
            }
            Some(_) if agent => Ok(()),
            _ => Err(Error::Invalid(NO_AGENT.to_owned())),
        }
    }

    async fn command(&mut self, command: Request) -> Option<String> {
        match command {
            Request::Subscribe(reply) => {
                if reply.is_closed() {
                    return None;
                }
                let result = match self.emulator.checkpoint_data(true, u64::MAX).await {
                    Ok(cut) => self.subscribers.subscribe(
                        self.incarnation,
                        cut.checkpoint,
                        cut.applied_sequence,
                    ),
                    Err(error) => Err(error),
                };
                if let Err(Ok(subscription)) = reply.send(result) {
                    self.subscribers.remove(subscription.connection_id);
                }
                self.note_local_view();
            }
            Request::Checkpoint { connection, reply } => {
                if reply.is_closed() {
                    return None;
                }
                let result = self.capture_for(connection).await;
                drop(reply.send(result));
            }
            Request::Unsubscribe(connection) => {
                self.subscribers.remove(connection);
                self.note_local_view();
                return self.release(Controller::Local(connection)).await;
            }
            Request::Input { connection, write } => self.accept_input(connection, write),
            Request::Send(write, message) => self.accept_command_input(write, message),
            Request::Read { history, reply } => {
                if !reply.is_closed() {
                    drop(reply.send(self.emulator.text(history).await));
                }
            }
            Request::Resize {
                connection,
                size,
                geometry,
                claim,
                reply,
            } => {
                return self
                    .local_resize(connection, size, geometry, claim, reply)
                    .await;
            }
            Request::Focus {
                connection,
                focused,
                reply,
            } => {
                if reply.is_closed() {
                    return None;
                }
                let (result, diverged) = match self.subscribers.authorization(connection) {
                    Ok(authorization) => {
                        self.focus_view(Controller::Local(connection), authorization, focused)
                            .await
                    }
                    Err(error) => (Err(error), false),
                };
                drop(reply.send(result));
                if diverged {
                    return Some(SIZE_NOT_APPLIED.to_owned());
                }
            }
            Request::Interrupt(reply) => {
                if !reply.is_closed() {
                    drop(
                        reply.send(
                            self.pty
                                .request_shutdown(ShutdownStage::Interrupt)
                                .map_err(Error::from),
                        ),
                    );
                }
            }
            Request::Run { command, reply } => {
                let result = if self.at_prompt(self.live_program().as_deref()) {
                    self.enqueue(Bytes::from(format!("{}{command}\r", self.clear_line)), None)
                } else {
                    Err(Error::Invalid("This terminal runs a program.".to_owned()))
                };
                drop(reply.send(result));
            }
            Request::Start(line) => {
                self.start = Some((line, tokio::time::Instant::now()));
            }
            Request::Foreground { variable, reply } => {
                drop(reply.send(Ok(Foreground {
                    group: self.pty.foreground_process_group(),
                    variable: self.pty.foreground_variable(variable),
                })));
            }
        }
        None
    }

    async fn capture_for(&self, connection: Uuid) -> Result<CheckpointCut> {
        let authorization = self.subscribers.authorization(connection)?;
        let cut = self.emulator.checkpoint_data(true, u64::MAX).await?;
        if authorization.is_cancelled() {
            return Err(Error::Stale);
        }
        Ok(CheckpointCut {
            checkpoint: self.metadata_checkpoint(cut.checkpoint),
            next_sequence: cut.applied_sequence,
        })
    }

    async fn local_resize(
        &mut self,
        connection: Uuid,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
        reply: oneshot::Sender<Result<()>>,
    ) -> Option<String> {
        if reply.is_closed() {
            return None;
        }
        let result = match self.subscribers.authorization(connection) {
            Ok(authorization) => {
                self.apply_resize(
                    Controller::Local(connection),
                    authorization,
                    size,
                    geometry,
                    claim,
                )
                .await
            }
            Err(error) => Err(ResizeError::Rejected(error)),
        };
        let (result, fatal) = match result {
            Ok(()) => (Ok(()), false),
            Err(error) => {
                let (error, fatal) = error.split();
                (Err(error), fatal)
            }
        };
        drop(reply.send(result));
        fatal.then(|| SIZE_NOT_APPLIED.to_owned())
    }

    async fn host_request(&mut self, request: HostRequest) -> Option<String> {
        match request {
            HostRequest::Bootstrap {
                request_id,
                history,
                image_data_bytes,
                reply,
            } => {
                if !reply.is_closed() {
                    let result = self
                        .emulator
                        .checkpoint_data(history, image_data_bytes)
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|cut| {
                            self.output
                                .send(PublishedFrame::BootstrapBarrier { request_id })
                                .map_err(|_| "Terminal publication disconnected.".to_owned())?;
                            Ok(CheckpointCut {
                                checkpoint: self.metadata_checkpoint(cut.checkpoint),
                                next_sequence: cut.applied_sequence,
                            })
                        });
                    drop(reply.send(result));
                }
            }
            HostRequest::ResetPresence { owner } => {
                self.viewers.clear();
                self.owner = owner;
                self.local_view = self.subscribers.has_active();
                self.publish_presence();
            }
            HostRequest::Connected {
                connection_id,
                user_id,
            } => {
                if self.viewers.len() < crate::protocol::TERMINAL_VIEWERS_MOST
                    || self.viewers.contains_key(&connection_id)
                {
                    self.viewers.insert(connection_id, user_id);
                    self.publish_presence();
                }
            }
            HostRequest::Disconnected { connection_id } => {
                self.viewers.remove(&connection_id);
                self.publish_presence();
                return self.release(Controller::Remote(connection_id)).await;
            }
            HostRequest::Control {
                connection_id,
                authorization,
                control,
                reply,
                ..
            } => {
                return self
                    .host_control(connection_id, authorization, control, reply)
                    .await;
            }
        }
        None
    }

    async fn host_control(
        &mut self,
        connection_id: Uuid,
        authorization: CancellationToken,
        control: TerminalControl,
        reply: oneshot::Sender<std::result::Result<serde_json::Value, String>>,
    ) -> Option<String> {
        if authorization.is_cancelled() || reply.is_closed() {
            drop(reply.send(Err(
                "This terminal connection is no longer authorized.".to_owned(),
            )));
            return None;
        }
        let mut fatal = false;
        let message = matches!(control, TerminalControl::Message { .. });
        let result = match control {
            TerminalControl::Input { bytes } | TerminalControl::Message { bytes } => {
                match PendingWrite::new(Bytes::from(bytes), &self.input_budget, Some(authorization))
                {
                    Ok(mut write) => {
                        write.completion = Some(InputCompletion::Remote(reply));
                        if message {
                            self.accept_command_input(write, true);
                        } else {
                            self.typed_at = Some(tokio::time::Instant::now());
                            self.queue.push_back(write);
                        }
                        return None;
                    }
                    Err(error) => Err(error),
                }
            }
            TerminalControl::Resize {
                rows,
                cols,
                width_pixels,
                height_pixels,
                cell_width_pixels,
                cell_height_pixels,
                claim,
                ..
            } => {
                let geometry = host_geometry(
                    width_pixels,
                    height_pixels,
                    cell_width_pixels,
                    cell_height_pixels,
                );
                match (TerminalSize::new(rows, cols), geometry) {
                    (Ok(size), Ok(geometry)) => match self
                        .apply_resize(
                            Controller::Remote(connection_id),
                            authorization,
                            size,
                            geometry,
                            claim,
                        )
                        .await
                    {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            let (error, diverged) = error.split();
                            fatal = diverged;
                            Err(error)
                        }
                    },
                    (Err(error), _) => Err(Error::Invalid(error.to_string())),
                    (_, Err(error)) => Err(error),
                }
            }
            TerminalControl::Focus { focused } => {
                let (result, diverged) = self
                    .focus_view(Controller::Remote(connection_id), authorization, focused)
                    .await;
                fatal = diverged
                    || result
                        .as_ref()
                        .is_err_and(|error| !matches!(error, Error::Stale));
                result
            }
            TerminalControl::Interrupt => self
                .pty
                .request_shutdown(ShutdownStage::Interrupt)
                .map_err(Error::from),
            TerminalControl::Close => {
                self.host_stop_reply = Some(reply);
                return Some("The terminal was closed.".to_owned());
            }
        };
        let reason = result.as_ref().err().map(ToString::to_string);
        drop(
            reply.send(
                result
                    .map(|()| serde_json::json!({"accepted":true}))
                    .map_err(|error| error.to_string()),
            ),
        );
        if fatal {
            return Some(
                reason.unwrap_or_else(|| "The terminal could not apply the control.".to_owned()),
            );
        }
        None
    }

    async fn terminate(&mut self, buffer: &mut [u8]) -> std::result::Result<(), String> {
        let mut readable = true;
        for (stage, grace) in [
            (ShutdownStage::Hangup, Duration::from_millis(500)),
            (ShutdownStage::Terminate, Duration::from_secs(2)),
            (ShutdownStage::Force, Duration::from_secs(2)),
        ] {
            drop(self.pty.request_shutdown(stage));
            let deadline = tokio::time::Instant::now() + grace;
            loop {
                tokio::select! {
                    result = self.pty.wait_within(deadline.saturating_duration_since(tokio::time::Instant::now())) => match result {
                        Ok(WaitOutcome::Reaped) => return Ok(()),
                        Ok(WaitOutcome::TimedOut) => break,
                        Err(error) => return Err(format!("Process termination was not confirmed: {error}")),
                    },
                    read = self.reader.read(buffer), if readable => match read {
                        Ok(0) | Err(_) => readable = false,
                        Ok(count) => {
                            if self.apply_output(Bytes::copy_from_slice(&buffer[..count]), false).await.is_err() {
                                readable = false;
                            }
                        }
                    }
                }
            }
        }
        Err("Process termination was not confirmed in time.".to_owned())
    }

    async fn run(mut self) {
        let mut buffer = vec![0_u8; READ_BYTES];
        let mut upkeep = tokio::time::Instant::now() + BUSY_UPKEEP;
        let mut host_open = true;
        let reason = loop {
            if self.cancellation.is_cancelled() {
                break "The terminal stopped.".to_owned();
            }
            let input = self.pending_input();
            let kept_up = tokio::select! {
                () = self.cancellation.cancelled() => break "The terminal stopped.".to_owned(),
                command = self.commands.recv() => match command {
                    Some(command) => { if let Some(reason) = self.command(command).await { break reason; } false }
                    None => break "Kodosi stopped.".to_owned(),
                },
                request = self.host.recv(), if host_open => {
                    if let Some(request) = request {
                        if let Some(reason) = self.host_request(request).await { break reason; }
                    } else {
                        host_open = false;
                    }
                    false
                },
                transfer = self.reader.transfer(&mut buffer, input.as_deref()) => match transfer {
                    Ok(Transfer::Read(0)) => break "The terminal ended.".to_owned(),
                    Ok(Transfer::Read(count)) => { if let Err(error) = self.apply_output(Bytes::copy_from_slice(&buffer[..count]), true).await { break error.to_string(); } false }
                    Ok(Transfer::Wrote(count)) => { self.wrote(count); false }
                    Err(error) if error.raw_os_error() == Some(5) => break "The terminal ended.".to_owned(),
                    Err(error) => break error.to_string(),
                },
                () = tokio::time::sleep_until(upkeep) => { if let Some(reason) = self.maintain().await { break reason; } true }
            };
            let now = tokio::time::Instant::now();
            upkeep = if !kept_up {
                upkeep.min(now + BUSY_UPKEEP)
            } else if self.busy() {
                now + BUSY_UPKEEP
            } else {
                now + IDLE_UPKEEP
            };
        };
        self.cancellation.cancel();
        self.commands.close();
        self.host.close();
        self.queue.clear();
        while self.commands.try_recv().is_ok() {}
        while self.host.try_recv().is_ok() {}
        let completion = self.terminate(&mut buffer).await;
        if let Some(reply) = self.host_stop_reply.take() {
            drop(
                reply.send(
                    completion
                        .clone()
                        .map(|()| serde_json::json!({"stopped":true})),
                ),
            );
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            match tokio::time::timeout_at(deadline, self.reader.read(&mut buffer)).await {
                Ok(Ok(count)) if count > 0 => {
                    if self
                        .apply_output(Bytes::copy_from_slice(&buffer[..count]), false)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                _ => break,
            }
        }
        let reason = completion
            .as_ref()
            .err()
            .map_or_else(|| reason.clone(), |error| format!("{reason} {error}"));
        self.subscribers.close(&reason, self.sequence);
        drop(self.output.send(PublishedFrame::Closed {
            reason: reason.clone(),
            final_sequence: self.sequence,
        }));
        drop(self.emulator.shutdown().await);
        drop(self.pty);
        self.completed.send_replace(Some(completion));
        drop(
            self.changes
                .send(SessionChange::Ended {
                    id: self.id,
                    reason,
                })
                .await,
        );
    }
}

fn host_geometry(
    width: u32,
    height: u32,
    cell_width: u32,
    cell_height: u32,
) -> Result<Option<TerminalPixelGeometry>> {
    if [width, height, cell_width, cell_height] == [0; 4] {
        return Ok(None);
    }
    TerminalPixelGeometry::new(width, height, cell_width, cell_height)
        .map(Some)
        .map_err(|error| Error::Invalid(error.to_string()))
}

#[cfg(all(test, unix))]
#[path = "session_tests.rs"]
mod tests;

#[cfg(all(test, windows))]
#[path = "session_windows_tests.rs"]
mod windows_tests;
