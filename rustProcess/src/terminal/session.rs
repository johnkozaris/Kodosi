use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use kodosi_pty::{KodosiPty, RawFdAsyncReader, ShutdownStage, WaitOutcome};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::emulator::{ClientFocus, SessionTerminalHandle, TerminalEffect, TerminalHistoryPolicy};
use super::subscribers::Subscribers;
use super::{ControlFrame, DataFrame, Subscription, TerminalPixelGeometry, TerminalSize};
use crate::{
    Error, Result,
    network::{CheckpointCut, HostRequest, PublishedFrame, TerminalControl},
};

const COMMAND_CAPACITY: usize = 64;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
const READ_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

type Completion = Option<std::result::Result<(), String>>;

pub(crate) enum SessionChange {
    RemoteEnded {
        id: Uuid,
        instance_id: Uuid,
    },
    Presence {
        id: Uuid,
        users: Vec<String>,
    },
    Cwd {
        id: Uuid,
        path: PathBuf,
    },
    Program {
        id: Uuid,
        program: Option<String>,
    },
    Title {
        id: Uuid,
        title: String,
    },
    Bell {
        id: Uuid,
    },
    Notification {
        id: Uuid,
        title: String,
        body: String,
    },
    Ended {
        id: Uuid,
        reason: String,
    },
    Metadata {
        id: Uuid,
        incarnation: Uuid,
        instance_id: Uuid,
        metadata: super::TerminalMetadata,
    },
    RemoteClosed {
        id: Uuid,
        incarnation: Uuid,
        instance_id: Uuid,
        reason: String,
    },
}

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
        reply: Option<oneshot::Sender<Result<()>>>,
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
    Theme(bool),
}

#[derive(Clone)]
pub(crate) struct LocalSession {
    pub incarnation_id: Uuid,
    commands: mpsc::Sender<Request>,
    pub host_requests: mpsc::Sender<HostRequest>,
    output: broadcast::Sender<PublishedFrame>,
    cancellation: CancellationToken,
    completion: watch::Receiver<Completion>,
    input_budget: Arc<Semaphore>,
}

impl LocalSession {
    pub(crate) async fn spawn(
        id: Uuid,
        incarnation_id: Uuid,
        program: PathBuf,
        arguments: Vec<String>,
        directory: PathBuf,
        dark: bool,
        changes: mpsc::Sender<SessionChange>,
    ) -> Result<Self> {
        let size = TerminalSize::default();
        let (emulator, pty, reader) = tokio::task::spawn_blocking(move || -> Result<_> {
            let cwd = directory
                .to_str()
                .ok_or_else(|| Error::Invalid("working directory must be UTF-8".to_owned()))?;
            let emulator =
                SessionTerminalHandle::spawn(size, TerminalHistoryPolicy::default(), dark)?;
            let (pty, reader) = KodosiPty::spawn_program(
                &program,
                &arguments,
                Some(cwd),
                size.rows(),
                size.cols(),
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
            sequence: 0,
            host_stop_reply: None,
            program: None,
            viewers: HashMap::new(),
            working_directory: None,
            title: None,
            published_title: None,
            metadata_dirty: false,
            next_program_check: tokio::time::Instant::now() + Duration::from_secs(1),
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

    #[cfg(test)]
    pub(crate) fn input(&self, connection: Uuid, bytes: Bytes) -> Result<()> {
        let write = PendingWrite::new(bytes, &self.input_budget, None)?;
        self.send(Request::Input {
            connection,
            write,
            reply: None,
        })
    }

    pub(crate) fn admit_input(
        &self,
        connection: Uuid,
        bytes: Bytes,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        let admission = PendingWrite::new(bytes, &self.input_budget, None).and_then(|write| {
            self.send(Request::Input {
                connection,
                write,
                reply: Some(reply),
            })
        });
        await_reply(admission, response)
    }

    pub(crate) fn resize(
        &self,
        connection: Uuid,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, response) = oneshot::channel();
        let admission = validated_pixels(size, geometry).and_then(|_| {
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

    pub(crate) fn theme(&self, dark: bool) {
        drop(self.send(Request::Theme(dark)));
    }

    pub(crate) fn stop(&self) -> impl Future<Output = Result<()>> + Send + use<> {
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

struct PendingWrite {
    bytes: Bytes,
    offset: usize,
    authorization: Option<CancellationToken>,
    _budget: OwnedSemaphorePermit,
}

impl PendingWrite {
    fn new(
        bytes: Bytes,
        budget: &Arc<Semaphore>,
        authorization: Option<CancellationToken>,
    ) -> Result<Self> {
        if bytes.len() > MAX_INPUT_BYTES {
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
            _budget: permit,
        })
    }

    fn is_cancelled(&self) -> bool {
        self.authorization
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Controller {
    Local(Uuid),
    Remote(Uuid),
}

impl Controller {
    fn focus_key(self) -> String {
        match self {
            Self::Local(id) => format!("local:{id}"),
            Self::Remote(id) => format!("remote:{id}"),
        }
    }
}

struct ResizeOwner {
    controller: Controller,
    authorization: CancellationToken,
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

fn validated_pixels(
    size: TerminalSize,
    geometry: Option<TerminalPixelGeometry>,
) -> Result<(Option<u16>, Option<u16>)> {
    let Some(geometry) = geometry else {
        return Ok((None, None));
    };
    let geometry = geometry
        .validate_for_size(size)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let width = u16::try_from(geometry.width_pixels())
        .map_err(|_| Error::Invalid("terminal pixel width exceeds the PTY limit".to_owned()))?;
    let height = u16::try_from(geometry.height_pixels())
        .map_err(|_| Error::Invalid("terminal pixel height exceeds the PTY limit".to_owned()))?;
    Ok((Some(width), Some(height)))
}

struct LocalActor {
    id: Uuid,
    incarnation: Uuid,
    pty: KodosiPty,
    reader: RawFdAsyncReader,
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
    sequence: u64,
    host_stop_reply: Option<oneshot::Sender<std::result::Result<serde_json::Value, String>>>,
    program: Option<String>,
    viewers: HashMap<Uuid, String>,
    working_directory: Option<PathBuf>,
    title: Option<String>,
    published_title: Option<String>,
    metadata_dirty: bool,
    next_program_check: tokio::time::Instant,
}

impl LocalActor {
    fn connected_users(&self) -> Vec<String> {
        self.viewers
            .values()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn publish_presence(&mut self) {
        self.metadata_dirty = true;
        drop(self.changes.try_send(SessionChange::Presence {
            id: self.id,
            users: self.connected_users(),
        }));
    }

    fn metadata_checkpoint(&self, mut checkpoint: super::Checkpoint) -> super::Checkpoint {
        checkpoint.metadata = Some(super::TerminalMetadata {
            connected_users: self.connected_users(),
            directory: self
                .pty
                .working_directory()
                .map(|path| path.to_string_lossy().into_owned()),
            title: self
                .title
                .as_ref()
                .map(|title| title.chars().take(1024).collect()),
            program: self.pty.foreground_program(),
        });
        checkpoint
    }

    fn enqueue(&mut self, bytes: Bytes, authorization: Option<CancellationToken>) -> Result<()> {
        self.queue
            .push_back(PendingWrite::new(bytes, &self.input_budget, authorization)?);
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        while let Some(write) = self.queue.front_mut() {
            if write.is_cancelled() || write.offset == write.bytes.len() {
                self.queue.pop_front();
                continue;
            }
            let count = self.pty.write(&write.bytes[write.offset..])?;
            if count == 0 {
                break;
            }
            write.offset += count;
            if write.offset == write.bytes.len() {
                self.queue.pop_front();
            }
        }
        Ok(())
    }

    async fn apply_resize(
        &mut self,
        controller: Controller,
        authorization: CancellationToken,
        size: TerminalSize,
        geometry: Option<TerminalPixelGeometry>,
        claim: bool,
    ) -> std::result::Result<(), ResizeError> {
        let (width, height) = validated_pixels(size, geometry).map_err(ResizeError::Rejected)?;
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
            .resize(size.rows(), size.cols(), geometry)
            .await
            .map_err(Error::from)
            .map_err(ResizeError::Diverged)?;
        for bytes in effects {
            self.enqueue(Bytes::from(bytes), None)
                .map_err(ResizeError::Diverged)?;
        }
        let cut = self
            .emulator
            .checkpoint_data()
            .await
            .map_err(Error::from)
            .map_err(ResizeError::Diverged)?;
        if cut.applied_sequence != self.sequence {
            return Err(ResizeError::Diverged(Error::Other(
                "terminal resize checkpoint is out of order".to_owned(),
            )));
        }
        self.resize_owner = Some(ResizeOwner {
            controller,
            authorization,
        });
        self.subscribers.control(&ControlFrame::Resize {
            rows: size.rows(),
            cols: size.cols(),
            at_sequence: self.sequence,
        });
        drop(self.output.send(PublishedFrame::Checkpoint {
            checkpoint: self.metadata_checkpoint(cut.checkpoint),
            next_sequence: self.sequence,
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
        let effects = self
            .emulator
            .set_client_focus(
                controller.focus_key(),
                if focused {
                    ClientFocus::Focused
                } else {
                    ClientFocus::Blurred
                },
            )
            .await?;
        if let Some(authorization) = &authorization {
            if focused {
                self.focused.insert(controller, authorization.clone());
            } else {
                self.focused.remove(&controller);
            }
        } else {
            self.focused.remove(&controller);
        }
        for bytes in effects {
            self.enqueue(
                Bytes::from(bytes),
                if focused { authorization.clone() } else { None },
            )?;
        }
        Ok(())
    }

    async fn release(&mut self, controller: Controller) -> Result<()> {
        if self.focused.remove(&controller).is_some() {
            self.set_focus(controller, None, false).await?;
        }
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|owner| owner.controller == controller)
        {
            self.resize_owner = None;
        }
        Ok(())
    }

    async fn maintain(&mut self) -> Result<()> {
        self.subscribers.prune();
        if tokio::time::Instant::now() >= self.next_program_check {
            self.next_program_check = tokio::time::Instant::now() + Duration::from_secs(1);
            if let Some(path) = self.pty.working_directory()
                && self.working_directory.as_ref() != Some(&path)
                && self
                    .changes
                    .try_send(SessionChange::Cwd {
                        id: self.id,
                        path: path.clone(),
                    })
                    .is_ok()
            {
                self.working_directory = Some(path);
                self.metadata_dirty = true;
            }
            let program = self.pty.foreground_program();
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
        }
        if self.title != self.published_title
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
            drop(self.output.send(PublishedFrame::MetadataChanged));
        }
        let expired = self
            .focused
            .iter()
            .filter(|(_, token)| token.is_cancelled())
            .map(|(controller, _)| *controller)
            .collect::<Vec<_>>();
        for controller in expired {
            self.release(controller).await?;
        }
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|owner| owner.authorization.is_cancelled())
        {
            self.resize_owner = None;
        }
        self.queue.retain(|write| !write.is_cancelled());
        self.flush()
    }

    async fn apply_output(&mut self, bytes: Bytes, accepting_input: bool) -> Result<()> {
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
                    if accepting_input {
                        self.enqueue(Bytes::from(bytes), None)?;
                    }
                }
                TerminalEffect::Cwd(path) => {
                    drop(
                        self.changes
                            .try_send(SessionChange::Cwd { id: self.id, path }),
                    );
                }
                TerminalEffect::Title(title) => {
                    self.metadata_dirty |= self.title.as_ref() != Some(&title);
                    self.title = Some(title.clone());
                }
                TerminalEffect::Bell => {
                    drop(self.changes.try_send(SessionChange::Bell { id: self.id }));
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

    fn accept_input(
        &mut self,
        connection: Uuid,
        mut write: PendingWrite,
        reply: Option<oneshot::Sender<Result<()>>>,
    ) -> Option<String> {
        let Ok(authorization) = self.subscribers.authorization(connection) else {
            if let Some(reply) = reply {
                drop(reply.send(Err(Error::Stale)));
            }
            return None;
        };
        write.authorization = Some(authorization);
        self.queue.push_back(write);
        let result = self.flush();
        let failure = result.as_ref().err().map(ToString::to_string);
        if let Some(reply) = reply {
            drop(reply.send(result));
        }
        if failure.is_some() {
            return failure;
        }
        None
    }

    async fn command(&mut self, command: Request) -> Option<String> {
        match command {
            Request::Subscribe(reply) => {
                if reply.is_closed() {
                    return None;
                }
                let result = match self.emulator.checkpoint_data().await {
                    Ok(cut) => self.subscribers.subscribe(
                        self.incarnation,
                        cut.checkpoint,
                        cut.applied_sequence,
                    ),
                    Err(error) => Err(error.into()),
                };
                if let Err(Ok(subscription)) = reply.send(result) {
                    self.subscribers.remove(subscription.connection_id);
                }
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
                if let Err(error) = self.release(Controller::Local(connection)).await {
                    return Some(error.to_string());
                }
            }
            Request::Input {
                connection,
                write,
                reply,
            } => return self.accept_input(connection, write, reply),
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
                let result = match self.subscribers.authorization(connection) {
                    Ok(authorization) => {
                        self.set_focus(Controller::Local(connection), Some(authorization), focused)
                            .await
                    }
                    Err(error) => Err(error),
                };
                let fatal = result
                    .as_ref()
                    .is_err_and(|error| !matches!(error, Error::Stale));
                drop(reply.send(result));
                if fatal {
                    return Some("The terminal could not apply its focus state.".to_owned());
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
            Request::Theme(dark) => {
                if let Err(error) = self.emulator.notify_theme_changed(dark).await {
                    return Some(error.to_string());
                }
            }
        }
        None
    }

    async fn capture_for(&self, connection: Uuid) -> Result<CheckpointCut> {
        let authorization = self.subscribers.authorization(connection)?;
        let cut = self.emulator.checkpoint_data().await?;
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
        fatal.then(|| "The terminal could not apply its new size.".to_owned())
    }

    async fn host_request(&mut self, request: HostRequest) -> Option<String> {
        match request {
            HostRequest::Bootstrap { request_id, reply } => {
                if !reply.is_closed() {
                    let result = self
                        .emulator
                        .checkpoint_data()
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
            HostRequest::ResetPresence => {
                self.viewers.clear();
                self.publish_presence();
            }
            HostRequest::Connected {
                connection_id,
                user_id,
            } => {
                if self.viewers.len() < 64 || self.viewers.contains_key(&connection_id) {
                    self.viewers.insert(connection_id, user_id);
                    self.publish_presence();
                }
            }
            HostRequest::Disconnected { connection_id } => {
                self.viewers.remove(&connection_id);
                self.publish_presence();
                if let Err(error) = self.release(Controller::Remote(connection_id)).await {
                    return Some(error.to_string());
                }
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
        let result = match control {
            TerminalControl::Input { bytes } => {
                let result = self
                    .enqueue(Bytes::from(bytes), Some(authorization))
                    .and_then(|()| self.flush());
                fatal = matches!(result, Err(Error::Io(_) | Error::Other(_)));
                result
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
                let result = self
                    .set_focus(
                        Controller::Remote(connection_id),
                        Some(authorization),
                        focused,
                    )
                    .await;
                fatal = result
                    .as_ref()
                    .is_err_and(|error| !matches!(error, Error::Stale));
                result
            }
            TerminalControl::Interrupt => self
                .pty
                .request_shutdown(ShutdownStage::Interrupt)
                .map_err(Error::from),
            TerminalControl::Stop => {
                self.host_stop_reply = Some(reply);
                return Some("Session stopped by a controller.".to_owned());
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
                        Ok(WaitOutcome::Reaped(_)) => return Ok(()),
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
        let mut tick = tokio::time::interval(Duration::from_millis(20));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut host_open = true;
        let reason = loop {
            if self.cancellation.is_cancelled() {
                break "Session stopped.".to_owned();
            }
            tokio::select! {
                () = self.cancellation.cancelled() => break "Session stopped.".to_owned(),
                command = self.commands.recv() => match command {
                    Some(command) => { if let Some(reason) = self.command(command).await { break reason; } }
                    None => break "Hosting runtime stopped.".to_owned(),
                },
                request = self.host.recv(), if host_open => match request {
                    Some(request) => { if let Some(reason) = self.host_request(request).await { break reason; } }
                    None => host_open = false,
                },
                read = self.reader.read(&mut buffer) => match read {
                    Ok(0) => break "Session ended.".to_owned(),
                    Ok(count) => { if let Err(error) = self.apply_output(Bytes::copy_from_slice(&buffer[..count]), true).await { break error.to_string(); } }
                    Err(error) if error.raw_os_error() == Some(5) => break "Session ended.".to_owned(),
                    Err(error) => break error.to_string(),
                },
                _ = tick.tick() => { if let Err(error) = self.maintain().await { break error.to_string(); } }
            }
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

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
