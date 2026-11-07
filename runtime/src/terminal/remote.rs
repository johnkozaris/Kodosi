use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use futures_util::FutureExt as _;
use serde_json::Value;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::subscribers::{DATA_CAPACITY, MAX_RAW_BYTES, Subscribers};
use super::validation::validate_terminal_checkpoint;
use super::{
    CellPixels, Checkpoint, ControlFrame, DataFrame, Picture, SessionChange, Subscription,
    TerminalPixelGeometry, TerminalSize,
};
use crate::{
    Error, Result,
    network::{
        CONTROL_TIMEOUT, CheckpointCut, RemoteConnection, RemoteControl, RemoteUpdate,
        TerminalControl,
    },
    protocol::PictureState,
    text::Text,
};

const CONTROL_CAPACITY: usize = 64;
const PICTURES_WAITING: usize = 8;
const INPUT_BUDGET: usize = 4 * 1024 * 1024;
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_WAIT: Duration = Duration::from_secs(10);
const MERGED_INPUT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub(crate) struct RemoteTerminal {
    requests: mpsc::Sender<Request>,
    cancellation: CancellationToken,
    input_budget: Arc<Semaphore>,
    link: Arc<AtomicU8>,
    hidden: Arc<AtomicBool>,
    pub incarnation_id: Uuid,
    pub instance_id: Uuid,
}

enum Request {
    Reconnected(Box<RemoteConnection>),
    Subscribe(oneshot::Sender<Result<Subscription>>),
    Checkpoint {
        connection: Uuid,
        reply: oneshot::Sender<Result<CheckpointCut>>,
    },
    Unsubscribe(Uuid),
    Control(QueuedControl),
    Canvas {
        request: crate::canvas::CanvasRequest,
        reply: oneshot::Sender<Result<Value>>,
    },
    Document {
        pipe: Uuid,
        key: String,
        name: String,
        reply: oneshot::Sender<Result<crate::canvas::PipeEnds>>,
    },
    Picture {
        picture: Picture,
        reply: oneshot::Sender<Result<()>>,
    },
}

struct QueuedControl {
    connection: Option<Uuid>,
    control: TerminalControl,
    reply: Option<oneshot::Sender<Result<Value>>>,
    budget: Option<OwnedSemaphorePermit>,
}

impl RemoteTerminal {
    pub(crate) fn spawn(
        connection: RemoteConnection,
        changes: mpsc::Sender<SessionChange>,
    ) -> Self {
        let incarnation_id = connection.session.incarnation_id;
        let instance_id = Uuid::now_v7();
        let (requests, receiver) = mpsc::channel(CONTROL_CAPACITY);
        let cancellation = CancellationToken::new();
        let input_budget = Arc::new(Semaphore::new(INPUT_BUDGET));
        let link = Arc::new(AtomicU8::new(Link::Open as u8));
        let hidden = Arc::new(AtomicBool::new(false));
        let terminal = Self {
            requests,
            cancellation: cancellation.clone(),
            input_budget,
            link: Arc::clone(&link),
            hidden: Arc::clone(&hidden),
            incarnation_id,
            instance_id,
        };
        let mut actor = RemoteActor::new(connection, receiver, cancellation, changes, instance_id);
        actor.shared_link = link;
        actor.hidden = hidden;
        tokio::spawn(actor.run());
        terminal
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.cancellation.is_cancelled() || self.requests.is_closed()
    }

    pub(crate) fn is_interrupted(&self) -> bool {
        self.link.load(Ordering::Acquire) != Link::Open as u8
    }

    pub(crate) fn is_paused(&self) -> bool {
        let link = self.link.load(Ordering::Acquire);
        link == Link::Paused as u8 || link == Link::Waking as u8
    }

    pub(crate) fn show(&self, shown: bool) {
        self.hidden.store(!shown, Ordering::Release);
    }

    pub(crate) fn resume(&self, connection: RemoteConnection) -> bool {
        self.send(Request::Reconnected(Box::new(connection)))
            .is_ok()
    }

    pub(crate) fn subscribe(&self) -> impl Future<Output = Result<Subscription>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        wait_for(
            self.send(Request::Subscribe(reply)),
            result,
            BOOTSTRAP_TIMEOUT + Duration::from_secs(1),
        )
    }

    pub(crate) fn checkpoint(
        &self,
        connection: Uuid,
    ) -> impl Future<Output = Result<CheckpointCut>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        wait_for(
            self.send(Request::Checkpoint { connection, reply }),
            result,
            BOOTSTRAP_TIMEOUT + Duration::from_secs(1),
        )
    }

    pub(crate) fn unsubscribe(&self, connection: Uuid) {
        drop(self.send(Request::Unsubscribe(connection)));
    }

    pub(crate) fn write_input(
        &self,
        connection: Uuid,
        bytes: Bytes,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        let admission = self.queue_input(Some(connection), bytes, Some(reply));
        async move {
            wait_for(admission, result, CONTROL_TIMEOUT)
                .await
                .map(|_| ())
        }
    }

    #[cfg(test)]
    pub(crate) fn input(&self, connection: Uuid, bytes: Bytes) -> Result<()> {
        self.queue_input(Some(connection), bytes, None)
    }

    pub(crate) fn read(
        &self,
        history: bool,
    ) -> impl Future<Output = Result<String>> + Send + use<> {
        let subscribing = self.subscribe();
        let terminal = self.clone();
        async move {
            let subscription = subscribing.await?;
            let policy = ghostty_vt::TerminalPolicy {
                console: subscription.checkpoint.console,
                ..ghostty_vt::TerminalPolicy::default()
            };
            let text =
                ghostty_vt::Terminal::from_snapshot(subscription.checkpoint.snapshot(), policy)
                    .and_then(|mut restored| restored.text(history))
                    .map_err(|error| Error::Other(error.to_string()));
            terminal.unsubscribe(subscription.connection_id);
            text
        }
    }

    pub(crate) fn send_input(
        &self,
        bytes: Bytes,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        let admission = self.queue_input(None, bytes, Some(reply));
        async move {
            wait_for(admission, result, CONTROL_TIMEOUT)
                .await
                .map(|_| ())
        }
    }

    fn queue_input(
        &self,
        connection: Option<Uuid>,
        bytes: Bytes,
        reply: Option<oneshot::Sender<Result<Value>>>,
    ) -> Result<()> {
        if bytes.len() > super::TERMINAL_INPUT_MAX_BYTES {
            return Err(Error::Invalid("terminal input exceeds 1 MiB".to_owned()));
        }
        let count = u32::try_from(bytes.len()).map_err(|_| Error::Busy)?;
        let budget = Arc::clone(&self.input_budget)
            .try_acquire_many_owned(count)
            .map_err(|_| Error::Busy)?;
        self.send(Request::Control(QueuedControl {
            connection,
            control: TerminalControl::Input {
                bytes: bytes.into(),
            },
            reply,
            budget: Some(budget),
        }))
    }

    pub(crate) fn control(
        &self,
        connection: Option<Uuid>,
        control: TerminalControl,
    ) -> impl Future<Output = Result<Value>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        let admission = if matches!(control, TerminalControl::Input { .. }) {
            Err(Error::Invalid(
                "Raw input requires a terminal subscription.".to_owned(),
            ))
        } else {
            self.send(Request::Control(QueuedControl {
                connection,
                control,
                reply: Some(reply),
                budget: None,
            }))
        };
        wait_for(admission, result, CONTROL_TIMEOUT)
    }

    /// Asks the canvas of the terminal. Canvas requests do not wait for the terminal's own
    /// operations, which keep their order.
    pub(crate) fn canvas(
        &self,
        request: crate::canvas::CanvasRequest,
    ) -> impl Future<Output = Result<Value>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        wait_for(
            self.send(Request::Canvas { request, reply }),
            result,
            CONTROL_TIMEOUT,
        )
    }

    /// Joins a live document on the computer of this terminal.
    pub(crate) fn document(
        &self,
        pipe: Uuid,
        key: String,
        name: String,
    ) -> impl Future<Output = Result<crate::canvas::PipeEnds>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        wait_for(
            self.send(Request::Document {
                pipe,
                key,
                name,
                reply,
            }),
            result,
            CONTROL_TIMEOUT,
        )
    }

    /// Sends a pasted picture to the terminal's computer, after the pictures that wait. A
    /// picture that waits or travels already is not sent again.
    pub(crate) fn picture(
        &self,
        picture: Picture,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        wait_for(
            self.send(Request::Picture { picture, reply }),
            result,
            CONTROL_TIMEOUT,
        )
    }

    pub(crate) fn disconnect(&self) {
        self.cancellation.cancel();
    }

    fn send(&self, request: Request) -> Result<()> {
        if self.is_closed() {
            return Err(Error::Stopped);
        }
        self.requests
            .try_send(request)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Stopped,
            })
    }
}

async fn wait_for<T>(
    admission: Result<()>,
    reply: oneshot::Receiver<Result<T>>,
    timeout: Duration,
) -> Result<T> {
    admission?;
    tokio::time::timeout(timeout, reply)
        .await
        .map_err(|_| {
            Error::Text(text!(
                "The remote terminal did not confirm the request, and Kodosi did not send it again."
            ))
        })?
        .map_err(|_| Error::Stopped)?
}

enum CaptureReply {
    Subscribe(oneshot::Sender<Result<Subscription>>),
    Checkpoint {
        connection: Uuid,
        reply: oneshot::Sender<Result<CheckpointCut>>,
    },
}

impl CaptureReply {
    fn is_closed(&self) -> bool {
        match self {
            Self::Subscribe(reply) => reply.is_closed(),
            Self::Checkpoint { reply, .. } => reply.is_closed(),
        }
    }

    fn reject(self, error: Error) {
        match self {
            Self::Subscribe(reply) => {
                drop(reply.send(Err(error)));
            }
            Self::Checkpoint { reply, .. } => {
                drop(reply.send(Err(error)));
            }
        }
    }
}

struct PendingCapture {
    reply: CaptureReply,
    minimum_cut: u64,
    deadline: tokio::time::Instant,
}

struct RequestedResize {
    control: TerminalControl,
    taken_turn: u64,
}

impl RequestedResize {
    fn size(&self) -> Option<(u16, u16)> {
        match self.control {
            TerminalControl::Resize { rows, cols, .. } => Some((rows, cols)),
            _ => None,
        }
    }
}

/// The picture that travels to the terminal's computer.
struct Sending {
    digest: [u8; 32],
    total: u64,
    result: Pin<Box<dyn Future<Output = crate::network::Result<()>> + Send>>,
}

struct InFlight {
    result: Pin<Box<dyn Future<Output = crate::network::Result<Value>> + Send>>,
    replies: Vec<oneshot::Sender<Result<Value>>>,
    input: bool,
    focus: Option<bool>,
    resize_owner: Option<(Uuid, CancellationToken)>,
    _budgets: Vec<OwnedSemaphorePermit>,
}

enum Step {
    Next,
    Failed(Text),
    Stop(Text),
    Ended,
}

impl From<Result<()>> for Step {
    fn from(outcome: Result<()>) -> Self {
        outcome.map_or_else(|error| Self::Failed(error.text()), |()| Self::Next)
    }
}

fn reconnecting() -> Text {
    text!("The connection to the other computer is not back yet, so Kodosi did not send this.")
}

fn reject(request: Request, error: Error) {
    match request {
        Request::Canvas { reply, .. } => drop(reply.send(Err(error))),
        Request::Document { reply, .. } => drop(reply.send(Err(error))),
        _ => {}
    }
}

fn wakes(request: &Request) -> bool {
    match request {
        Request::Control(queued) => !matches!(
            queued.control,
            TerminalControl::Resize { .. } | TerminalControl::Focus { .. }
        ),
        Request::Subscribe(_)
        | Request::Checkpoint { .. }
        | Request::Canvas { .. }
        | Request::Document { .. }
        | Request::Picture { .. } => true,
        Request::Reconnected(_) | Request::Unsubscribe(_) => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Link {
    Open,
    Lost,
    Paused,
    Waking,
}

struct RemoteActor {
    connection: RemoteConnection,
    controls: RemoteControl,
    requests: mpsc::Receiver<Request>,
    cancellation: CancellationToken,
    changes: mpsc::Sender<SessionChange>,
    instance_id: Uuid,
    subscribers: Subscribers,
    pending: Vec<PendingCapture>,
    metadata: Option<super::TerminalMetadata>,
    capture_requested: bool,
    capture_due: tokio::time::Instant,
    next_sequence: Option<u64>,
    delivered: u64,
    recent: VecDeque<DataFrame>,
    grid: Option<(TerminalSize, CellPixels)>,
    last_resize: Option<u64>,
    future_checkpoint: Option<CheckpointCut>,
    resyncing: bool,
    shared_link: Arc<AtomicU8>,
    hidden: Arc<AtomicBool>,
    link: Link,
    held: VecDeque<Request>,
    asking: Arc<()>,
    documents: Vec<mpsc::WeakSender<Vec<u8>>>,
    queued: VecDeque<QueuedControl>,
    in_flight: Option<InFlight>,
    focused: HashSet<Uuid>,
    confirmed_focus: bool,
    resize_owner: Option<(Uuid, CancellationToken)>,
    requested: HashMap<Uuid, RequestedResize>,
    typed_at: Option<tokio::time::Instant>,
    lost_at: Option<tokio::time::Instant>,
    pictures: VecDeque<Picture>,
    sending: Option<Sending>,
}

impl RemoteActor {
    fn new(
        connection: RemoteConnection,
        requests: mpsc::Receiver<Request>,
        cancellation: CancellationToken,
        changes: mpsc::Sender<SessionChange>,
        instance_id: Uuid,
    ) -> Self {
        let controls = connection.control_handle();
        Self {
            connection,
            controls,
            requests,
            cancellation,
            changes,
            instance_id,
            subscribers: Subscribers::default(),
            pending: Vec::new(),
            metadata: None,
            capture_requested: false,
            capture_due: tokio::time::Instant::now(),
            next_sequence: None,
            delivered: 0,
            recent: VecDeque::new(),
            grid: None,
            last_resize: None,
            future_checkpoint: None,
            resyncing: false,
            shared_link: Arc::new(AtomicU8::new(Link::Open as u8)),
            hidden: Arc::new(AtomicBool::new(false)),
            link: Link::Open,
            held: VecDeque::new(),
            asking: Arc::new(()),
            documents: Vec::new(),
            queued: VecDeque::new(),
            in_flight: None,
            focused: HashSet::new(),
            confirmed_focus: false,
            resize_owner: None,
            requested: HashMap::new(),
            typed_at: None,
            lost_at: None,
            pictures: VecDeque::new(),
            sending: None,
        }
    }

    fn reset(&mut self) {
        if let Some(next) = self.next_sequence.take() {
            self.delivered = next;
        }
        self.recent.clear();
        self.future_checkpoint = None;
        self.last_resize = None;
        self.resyncing = true;
    }

    async fn interrupt(&mut self, reason: Text) -> Option<Text> {
        self.subscribers.prune();
        if self.subscribers.is_empty() {
            return Some(reason);
        }
        self.set_link(Link::Lost);
        self.lost_at.get_or_insert_with(tokio::time::Instant::now);
        self.connection.disconnect();
        let refuse = || Error::Text(reason.clone());
        for reply in self
            .in_flight
            .take()
            .into_iter()
            .flat_map(|active| active.replies)
        {
            drop(reply.send(Err(refuse())));
        }
        for waiting in self.pending.drain(..) {
            waiting.reply.reject(refuse());
        }
        if let Some(sending) = self.sending.take() {
            self.tell_picture(
                sending.total,
                PictureState::Failed,
                0,
                Some(text!(
                    "The connection to the other computer was lost, so the picture did not arrive. Paste it again."
                )),
            )
            .await;
        }
        self.reset();
        self.capture_requested = false;
        self.confirmed_focus = false;
        self.resize_owner = None;
        drop(
            self.changes
                .send(SessionChange::RemoteInterrupted {
                    id: self.connection.session.id,
                    incarnation: self.connection.session.incarnation_id,
                    instance_id: self.instance_id,
                    reason,
                })
                .await,
        );
        None
    }

    fn resume(&mut self, connection: RemoteConnection) {
        if connection.session.incarnation_id != self.connection.session.incarnation_id
            || self.link == Link::Open
        {
            connection.disconnect();
            return;
        }
        self.controls = connection.control_handle();
        self.connection = connection;
        self.set_link(Link::Open);
        self.lost_at = None;
    }

    fn hold(&mut self, request: Request) {
        if self.held.len() < CONTROL_CAPACITY {
            self.held.push_back(request);
        } else {
            reject(request, Error::Busy);
        }
    }

    fn forward(&mut self, request: Request) {
        match request {
            Request::Canvas { reply, .. } if reply.is_closed() => {}
            Request::Document { reply, .. } if reply.is_closed() => {}
            Request::Canvas { request, reply } => {
                let controls = self.controls.clone();
                let asking = Arc::clone(&self.asking);
                tokio::spawn(async move {
                    let answer = controls
                        .send_control(TerminalControl::Canvas { request })
                        .await;
                    drop(reply.send(answer.map_err(Error::from)));
                    drop(asking);
                });
            }
            Request::Document {
                pipe,
                key,
                name,
                reply,
            } => {
                let ends = self
                    .controls
                    .open_pipe(pipe, key, name)
                    .map_err(Error::from);
                if let Ok(ends) = &ends {
                    self.documents.push(ends.sender.downgrade());
                }
                drop(reply.send(ends));
            }
            _ => {}
        }
    }

    fn open_document(pipe: &mpsc::WeakSender<Vec<u8>>) -> bool {
        pipe.upgrade().is_some_and(|sender| !sender.is_closed())
    }

    fn set_link(&mut self, link: Link) {
        self.link = link;
        self.shared_link.store(link as u8, Ordering::Release);
    }

    fn dormant(&self) -> bool {
        matches!(self.link, Link::Paused | Link::Waking)
    }

    fn lost_too_long(&self) -> bool {
        self.link == Link::Lost
            && self
                .lost_at
                .is_some_and(|at| at.elapsed() >= RECONNECT_WAIT)
    }

    async fn wake(&mut self) {
        self.set_link(Link::Waking);
        drop(
            self.changes
                .send(SessionChange::RemoteWanted {
                    id: self.connection.session.id,
                    incarnation: self.connection.session.incarnation_id,
                    instance_id: self.instance_id,
                })
                .await,
        );
    }

    fn idle(&self) -> bool {
        self.in_flight.is_none()
            && self.queued.is_empty()
            && self.pictures.is_empty()
            && self.sending.is_none()
            && self.pending.is_empty()
            && self.held.is_empty()
            && self.requests.is_empty()
            && Arc::strong_count(&self.asking) == 1
            && !self.documents.iter().any(Self::open_document)
    }

    async fn pause(&mut self) {
        self.set_link(Link::Paused);
        self.connection.disconnect();
        self.reset();
        self.capture_requested = false;
        self.confirmed_focus = false;
        self.resize_owner = None;
        drop(
            self.changes
                .send(SessionChange::RemotePaused {
                    id: self.connection.session.id,
                    incarnation: self.connection.session.incarnation_id,
                    instance_id: self.instance_id,
                })
                .await,
        );
    }

    async fn request_capture(&mut self) -> Result<()> {
        if self.link != Link::Open {
            return Ok(());
        }
        if !self.capture_requested
            && self.future_checkpoint.is_none()
            && (!self.pending.is_empty() || self.subscribers.caught_up())
        {
            self.controls.request_checkpoint().await?;
            self.capture_requested = true;
            self.capture_due = tokio::time::Instant::now() + BOOTSTRAP_TIMEOUT;
        }
        Ok(())
    }

    async fn add_capture(&mut self, reply: CaptureReply) -> Result<()> {
        if self.pending.len() >= 32 {
            reply.reject(Error::Busy);
            return Ok(());
        }
        if let CaptureReply::Checkpoint { connection, .. } = &reply
            && !self.subscribers.contains(*connection)
        {
            reply.reject(Error::Stale);
            return Ok(());
        }
        self.pending.push(PendingCapture {
            reply,
            minimum_cut: self.next_sequence.unwrap_or(0),
            deadline: tokio::time::Instant::now() + BOOTSTRAP_TIMEOUT,
        });
        self.request_capture().await
    }

    fn geometry(&mut self, size: TerminalSize, cell: CellPixels, cut: u64) {
        if self.grid.is_some_and(|previous| previous != (size, cell)) {
            self.subscribers.control(&ControlFrame::Resize {
                rows: size.rows(),
                cols: size.cols(),
                cell,
                at_sequence: cut,
            });
            self.last_resize = Some(cut);
        }
        self.grid = Some((size, cell));
    }

    fn fulfill(&mut self, checkpoint: &Checkpoint, cut: u64) {
        let current = self.next_sequence.unwrap_or(cut);
        if cut == current {
            self.subscribers.refresh(checkpoint, cut);
        }
        let replayable = cut <= current
            && (cut == current
                || (self
                    .recent
                    .front()
                    .is_some_and(|frame| frame.sequence <= cut)
                    && self.last_resize.is_none_or(|sequence| sequence < cut)));
        let mut pending = Vec::new();
        for waiting in self.pending.drain(..) {
            if waiting.reply.is_closed() {
                continue;
            }
            if cut < waiting.minimum_cut || cut > current {
                pending.push(waiting);
                continue;
            }
            match waiting.reply {
                CaptureReply::Checkpoint { connection, reply } => {
                    let result =
                        self.subscribers
                            .authorization(connection)
                            .map(|_| CheckpointCut {
                                checkpoint: checkpoint.clone(),
                                next_sequence: cut,
                            });
                    drop(reply.send(result));
                }
                CaptureReply::Subscribe(reply) => {
                    if !replayable {
                        pending.push(PendingCapture {
                            reply: CaptureReply::Subscribe(reply),
                            ..waiting
                        });
                        continue;
                    }
                    let result = self.subscribers.subscribe(
                        self.connection.session.incarnation_id,
                        checkpoint.clone(),
                        cut,
                    );
                    match result {
                        Ok(subscription) => {
                            let id = subscription.connection_id;
                            let frames = self
                                .recent
                                .iter()
                                .filter(|frame| frame.sequence >= cut)
                                .cloned()
                                .collect::<Vec<_>>();
                            if !self.subscribers.replay(id, &frames) {
                                self.subscribers.remove(id);
                                drop(reply.send(Err(Error::Busy)));
                                continue;
                            }
                            if reply.send(Ok(subscription)).is_err() {
                                self.subscribers.remove(id);
                            }
                        }
                        Err(error) => {
                            drop(reply.send(Err(error)));
                        }
                    }
                }
            }
        }
        self.pending = pending;
    }

    fn update(&mut self, update: RemoteUpdate) -> Result<()> {
        match update {
            RemoteUpdate::Raw { sequence, bytes } => {
                let expected = self.next_sequence.ok_or_else(|| {
                    Error::Other("Remote output arrived before a terminal snapshot.".to_owned())
                })?;
                if sequence != expected || bytes.len() > MAX_RAW_BYTES {
                    return Err(Error::Other(
                        "Remote output is out of order or exceeds its buffer bound.".to_owned(),
                    ));
                }
                let next = sequence.checked_add(1).ok_or_else(|| {
                    Error::Other("Terminal output sequence exhausted.".to_owned())
                })?;
                let frame = DataFrame { sequence, bytes };
                self.recent.push_back(frame.clone());
                while self.recent.len() > DATA_CAPACITY {
                    self.recent.pop_front();
                }
                self.subscribers.data(&frame);
                self.next_sequence = Some(next);
                if self
                    .future_checkpoint
                    .as_ref()
                    .is_some_and(|cut| cut.next_sequence == next)
                    && let Some(cut) = self.future_checkpoint.take()
                {
                    self.geometry(cut.checkpoint.size(), cut.checkpoint.cell(), next);
                    self.fulfill(&cut.checkpoint, next);
                }
            }
            RemoteUpdate::Checkpoint {
                checkpoint,
                next_sequence: cut,
            } => {
                validate_terminal_checkpoint(&checkpoint)?;
                if let Some(metadata) = checkpoint.metadata.clone() {
                    self.metadata = Some(*metadata);
                }
                self.capture_requested = false;
                match self.next_sequence {
                    None => {
                        self.next_sequence = Some(cut);
                        if std::mem::take(&mut self.resyncing) {
                            self.subscribers.control(&ControlFrame::Snapshot {
                                checkpoint: checkpoint.clone(),
                                next_sequence: cut,
                            });
                        }
                        self.geometry(checkpoint.size(), checkpoint.cell(), cut);
                    }
                    Some(current) if cut > current => {
                        if self
                            .future_checkpoint
                            .as_ref()
                            .is_none_or(|existing| existing.next_sequence <= cut)
                        {
                            self.future_checkpoint = Some(CheckpointCut {
                                checkpoint,
                                next_sequence: cut,
                            });
                        }
                        return Ok(());
                    }
                    Some(current) if cut == current => {
                        self.geometry(checkpoint.size(), checkpoint.cell(), cut);
                    }
                    Some(_) => {}
                }
                self.fulfill(&checkpoint, cut);
            }
            RemoteUpdate::Metadata(metadata) => self.metadata = Some(metadata),
            RemoteUpdate::Resize {
                rows,
                cols,
                cell,
                at_sequence,
            } => {
                if self.next_sequence != Some(at_sequence) {
                    return Err(Error::Other(
                        "Remote resize arrived out of order.".to_owned(),
                    ));
                }
                let size = TerminalSize::new(rows, cols)
                    .map_err(|error| Error::Invalid(error.to_string()))?;
                self.geometry(size, cell, at_sequence);
            }
            RemoteUpdate::Resync => self.reset(),
            RemoteUpdate::Picture { sent, total } => self.picture_progress(sent, total),
            RemoteUpdate::Ended | RemoteUpdate::Closed { .. } => return Err(Error::Stopped),
        }
        Ok(())
    }

    fn queue(&mut self, queued: QueuedControl) -> Result<()> {
        if self.queued.len() >= CONTROL_CAPACITY {
            if let Some(reply) = queued.reply {
                drop(reply.send(Err(Error::Busy)));
                return Ok(());
            }
            return Err(Error::Text(text!(
                "Remote input exceeded its connection buffer, and Kodosi did not send it again."
            )));
        }
        let resize = match (&queued.connection, &queued.control) {
            (Some(_), TerminalControl::Input { .. }) => {
                self.typed_at = Some(tokio::time::Instant::now());
                None
            }
            (Some(id), TerminalControl::Focus { focused: true })
                if !self
                    .resize_owner
                    .as_ref()
                    .is_some_and(|(owner, token)| owner == id && !token.is_cancelled()) =>
            {
                self.requested
                    .get(id)
                    .map(|requested| (*id, requested.control.clone()))
            }
            (None, TerminalControl::Input { .. })
                if self.typed_at.is_some_and(|at| at.elapsed() < super::TYPING) =>
            {
                if let Some(reply) = queued.reply {
                    drop(reply.send(Err(Error::Text(super::someone_typing()))));
                }
                return Ok(());
            }
            _ => None,
        };
        self.queued.push_back(queued);
        if let Some((id, control)) = resize {
            self.queued.push_back(QueuedControl {
                connection: Some(id),
                control,
                reply: None,
                budget: None,
            });
        }
        Ok(())
    }

    fn next_control(&mut self) {
        if self.in_flight.is_some() || self.next_sequence.is_none() {
            return;
        }
        if let Some((owner, _)) = self.resize_owner.take_if(|(_, token)| token.is_cancelled()) {
            self.give_size_back(owner);
        }
        self.focused.retain(|id| self.subscribers.contains(*id));
        self.requested
            .retain(|id, _| self.subscribers.contains(*id));
        let aggregate = !self.focused.is_empty();
        if aggregate != self.confirmed_focus {
            self.begin(
                TerminalControl::Focus { focused: aggregate },
                Vec::new(),
                None,
                Vec::new(),
            );
            return;
        }
        while let Some(queued) = self.queued.pop_front() {
            if queued
                .reply
                .as_ref()
                .is_some_and(oneshot::Sender::is_closed)
            {
                continue;
            }
            let authorization = match queued
                .connection
                .map(|id| self.subscribers.authorization(id))
                .transpose()
            {
                Ok(authorization) => authorization,
                Err(error) => {
                    if let Some(reply) = queued.reply {
                        drop(reply.send(Err(error)));
                    }
                    continue;
                }
            };
            let mut resize_owner = None;
            let mut control = queued.control;
            let (mut replies, mut budgets) = (Vec::new(), Vec::new());
            match &mut control {
                TerminalControl::Focus { focused } => {
                    let Some(id) = queued.connection else {
                        if let Some(reply) = queued.reply {
                            drop(reply.send(Err(Error::Stale)));
                        }
                        continue;
                    };
                    if *focused {
                        self.focused.insert(id);
                    } else {
                        self.focused.remove(&id);
                    }
                    *focused = !self.focused.is_empty();
                    if *focused == self.confirmed_focus {
                        if let Some(reply) = queued.reply {
                            drop(reply.send(Ok(Value::Null)));
                        }
                        continue;
                    }
                }
                TerminalControl::Resize { .. } => {
                    let id = match self.check_resize(queued.connection, &mut control) {
                        Ok(id) => id,
                        Err(error) => {
                            if let Some(reply) = queued.reply {
                                drop(reply.send(Err(error)));
                            }
                            continue;
                        }
                    };
                    resize_owner = authorization.map(|token| (id, token));
                }
                TerminalControl::Input { bytes } => {
                    self.merge_input(queued.connection, bytes, &mut replies, &mut budgets);
                }
                TerminalControl::Message { .. }
                | TerminalControl::Interrupt
                | TerminalControl::Close
                | TerminalControl::Canvas { .. } => {}
            }
            if let Some(reply) = queued.reply {
                replies.insert(0, reply);
            }
            budgets.extend(queued.budget);
            self.begin(control, replies, resize_owner, budgets);
            break;
        }
    }

    fn give_size_back(&mut self, owner: Uuid) {
        let current = self.grid.map(|(size, _)| (size.rows(), size.cols()));
        if current.is_none()
            || self.requested.get(&owner).and_then(RequestedResize::size) != current
        {
            return;
        }
        let Some((id, control)) = self
            .focused
            .iter()
            .filter(|id| self.subscribers.contains(**id))
            .filter_map(|id| self.requested.get(id).map(|requested| (*id, requested)))
            .max_by_key(|(_, requested)| requested.taken_turn)
            .map(|(id, requested)| (id, requested.control.clone()))
        else {
            return;
        };
        let waiting = self.queued.iter().any(|queued| {
            queued.connection == Some(id)
                && matches!(queued.control, TerminalControl::Resize { .. })
        });
        if !waiting {
            self.queued.push_back(QueuedControl {
                connection: Some(id),
                control,
                reply: None,
                budget: None,
            });
        }
    }

    fn merge_input(
        &mut self,
        connection: Option<Uuid>,
        bytes: &mut Vec<u8>,
        replies: &mut Vec<oneshot::Sender<Result<Value>>>,
        budgets: &mut Vec<OwnedSemaphorePermit>,
    ) {
        while let Some(next) = self.queued.front()
            && next.connection == connection
            && let TerminalControl::Input { bytes: more } = &next.control
            && bytes.len() + more.len() <= MERGED_INPUT_BYTES
            && let Some(next) = self.queued.pop_front()
        {
            if next.reply.as_ref().is_some_and(oneshot::Sender::is_closed) {
                continue;
            }
            if let TerminalControl::Input { bytes: more } = next.control {
                bytes.extend(more);
            }
            replies.extend(next.reply);
            budgets.extend(next.budget);
        }
    }

    fn check_resize(
        &mut self,
        connection: Option<Uuid>,
        control: &mut TerminalControl,
    ) -> Result<Uuid> {
        let id = connection.ok_or(Error::Stale)?;
        let taken_turn = self
            .requested
            .get(&id)
            .map_or(0, |requested| requested.taken_turn);
        self.requested.insert(
            id,
            RequestedResize {
                control: control.clone(),
                taken_turn,
            },
        );
        let focused = self.focused.contains(&id);
        let TerminalControl::Resize {
            rows,
            cols,
            width_pixels,
            height_pixels,
            cell_width_pixels,
            cell_height_pixels,
            claim,
            ..
        } = control
        else {
            return Err(Error::Invalid("Expected a terminal resize.".to_owned()));
        };
        validate_resize(
            *rows,
            *cols,
            *width_pixels,
            *height_pixels,
            *cell_width_pixels,
            *cell_height_pixels,
        )?;
        *claim = *claim || focused;
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|(owner, token)| *owner != id && !token.is_cancelled())
            && !*claim
        {
            return Err(Error::Text(text!(
                "Another terminal view controls the size. Focus this view to resize it."
            )));
        }
        Ok(id)
    }

    fn begin(
        &mut self,
        control: TerminalControl,
        replies: Vec<oneshot::Sender<Result<Value>>>,
        resize_owner: Option<(Uuid, CancellationToken)>,
        budgets: Vec<OwnedSemaphorePermit>,
    ) {
        let focus = match &control {
            TerminalControl::Focus { focused } => Some(*focused),
            _ => None,
        };
        let input = matches!(control, TerminalControl::Input { .. });
        let controls = self.controls.clone();
        self.in_flight = Some(InFlight {
            result: Box::pin(async move { controls.send_control(control).await }),
            replies,
            input,
            focus,
            resize_owner,
            _budgets: budgets,
        });
    }

    fn finish_control(&mut self, result: crate::network::Result<Value>) -> Result<()> {
        let Some(active) = self.in_flight.take() else {
            return Err(Error::Stopped);
        };
        let mut fatal = None;
        if result.is_ok() {
            if let Some(focus) = active.focus {
                self.confirmed_focus = focus;
            }
            if let Some(owner) = active.resize_owner {
                let taken_turn = self
                    .requested
                    .values()
                    .map(|requested| requested.taken_turn)
                    .max()
                    .map_or(1, |latest| latest + 1);
                if let Some(requested) = self.requested.get_mut(&owner.0) {
                    requested.taken_turn = taken_turn;
                }
                self.resize_owner = Some(owner);
            }
        } else if active.input
            || active.focus.is_some()
            || matches!(
                result,
                Err(crate::network::Error::Closed | crate::network::Error::Stale)
            )
        {
            fatal = result.as_ref().err().map(|error| {
                text!(
                    "The remote terminal did not confirm the control ({error}), and Kodosi did not send it again.",
                    error = error.text()
                )
            });
        }
        let failure = result.as_ref().err().map(crate::network::Error::text);
        let mut replies = active.replies.into_iter();
        if let Some(first) = replies.next() {
            drop(first.send(result.map_err(Error::from)));
        }
        for reply in replies {
            drop(
                reply.send(
                    failure
                        .as_ref()
                        .map_or(Ok(Value::Null), |message| Err(Error::Text(message.clone()))),
                ),
            );
        }
        fatal.map_or(Ok(()), |reason| Err(Error::Text(reason)))
    }

    async fn maintain(&mut self) -> Result<()> {
        if let Some(metadata) = self.metadata.take()
            && let Err(mpsc::error::TrySendError::Full(SessionChange::Metadata {
                metadata, ..
            })) = self.changes.try_send(SessionChange::Metadata {
                id: self.connection.session.id,
                incarnation: self.connection.session.incarnation_id,
                instance_id: self.instance_id,
                metadata,
            })
        {
            self.metadata = Some(metadata);
        }
        self.subscribers.prune();
        self.documents.retain(Self::open_document);
        if self.lost_too_long() {
            for reply in self.queued.drain(..).filter_map(|queued| queued.reply) {
                drop(reply.send(Err(Error::Text(reconnecting()))));
            }
            for request in std::mem::take(&mut self.held) {
                reject(request, Error::Text(reconnecting()));
            }
        }
        let now = tokio::time::Instant::now();
        let mut pending = Vec::new();
        for waiting in self.pending.drain(..) {
            if waiting.reply.is_closed() {
                continue;
            }
            if now >= waiting.deadline {
                waiting.reply.reject(Error::Other(
                    "The host did not send a current terminal snapshot.".to_owned(),
                ));
            } else {
                pending.push(waiting);
            }
        }
        self.pending = pending;
        if now >= self.capture_due || self.pending.is_empty() && !self.subscribers.caught_up() {
            self.capture_requested = false;
        }
        self.request_capture().await
    }

    fn connection_update(&mut self, update: Option<RemoteUpdate>) -> Step {
        if matches!(
            &update,
            None | Some(RemoteUpdate::Closed { .. } | RemoteUpdate::Ended)
        ) && let Some(result) = self
            .in_flight
            .as_mut()
            .and_then(|active| active.result.as_mut().now_or_never())
            && let Err(error) = self.finish_control(result)
        {
            return Step::Failed(error.text());
        }
        match update {
            Some(RemoteUpdate::Ended) => Step::Ended,
            Some(RemoteUpdate::Closed { reason }) => Step::Failed(reason),
            Some(update) => Step::from(self.update(update)),
            None => Step::Failed(text!("The remote host disconnected.")),
        }
    }

    /// Tells how much of the picture arrived. A progress that does not fit the queue waits for
    /// the next one.
    fn picture_progress(&self, sent: u64, total: u64) {
        if self
            .sending
            .as_ref()
            .is_some_and(|sending| sending.total == total)
        {
            drop(self.changes.try_send(SessionChange::Picture {
                id: self.connection.session.id,
                incarnation: self.connection.session.incarnation_id,
                instance_id: self.instance_id,
                state: PictureState::Sending,
                sent,
                total,
                message: None,
            }));
        }
    }

    fn wait_picture(&mut self, picture: Picture) -> Result<()> {
        let digest = picture.digest;
        if self
            .sending
            .as_ref()
            .is_some_and(|sending| sending.digest == digest)
            || self.pictures.iter().any(|waiting| waiting.digest == digest)
        {
            return Ok(());
        }
        if self.pictures.len() >= PICTURES_WAITING {
            return Err(Error::Text(text!(
                "Kodosi still sends earlier pictures to this terminal. Paste this one in a moment."
            )));
        }
        self.pictures.push_back(picture);
        Ok(())
    }

    async fn next_picture(&mut self) {
        if self.sending.is_some() || self.next_sequence.is_none() {
            return;
        }
        let Some(picture) = self.pictures.pop_front() else {
            return;
        };
        let sending = Sending {
            digest: picture.digest,
            total: picture.bytes.len() as u64,
            result: Box::pin(self.controls.send_picture(picture)),
        };
        self.tell_picture(sending.total, PictureState::Sending, 0, None)
            .await;
        self.sending = Some(sending);
    }

    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "exclusive borrow keeps the actor future Send across the await"
    )]
    async fn tell_picture(
        &mut self,
        total: u64,
        state: PictureState,
        sent: u64,
        message: Option<Text>,
    ) {
        drop(
            self.changes
                .send(SessionChange::Picture {
                    id: self.connection.session.id,
                    incarnation: self.connection.session.incarnation_id,
                    instance_id: self.instance_id,
                    state,
                    sent,
                    total,
                    message,
                })
                .await,
        );
    }

    async fn step(&mut self, tick: &mut tokio::time::Interval) -> Step {
        let interrupted = self.link != Link::Open;
        if !interrupted && self.hidden.load(Ordering::Acquire) && self.idle() {
            self.pause().await;
            return Step::Next;
        }
        if !interrupted {
            for request in std::mem::take(&mut self.held) {
                self.forward(request);
            }
            self.next_control();
            self.next_picture().await;
        }
        let outcome = tokio::select! {
            () = self.cancellation.cancelled() => return Step::Stop(text!("Disconnected from the remote terminal.")),
            result = async { match self.in_flight.as_mut() { Some(active) => active.result.as_mut().await, None => std::future::pending().await } } => self.finish_control(result),
            result = async { match self.sending.as_mut() { Some(sending) => sending.result.as_mut().await, None => std::future::pending().await } } => {
                if let Some(sending) = self.sending.take() {
                    let (state, message) = match result {
                        Ok(()) => (PictureState::Sent, None),
                        Err(error) => (PictureState::Failed, Some(error.text())),
                    };
                    self.tell_picture(sending.total, state, sending.total, message).await;
                }
                Ok(())
            }
            update = self.connection.updates.recv(), if !interrupted => return self.connection_update(update),
            request = self.requests.recv() => {
                if request.as_ref().is_some_and(wakes) && self.dormant() {
                    self.wake().await;
                }
                match request {
                Some(Request::Reconnected(connection)) => { self.resume(*connection); Ok(()) }
                Some(Request::Subscribe(reply)) => self.add_capture(CaptureReply::Subscribe(reply)).await,
                Some(Request::Checkpoint { connection, reply }) => self.add_capture(CaptureReply::Checkpoint { connection, reply }).await,
                Some(Request::Unsubscribe(id)) => { self.subscribers.remove(id); self.focused.remove(&id); Ok(()) }
                Some(Request::Control(queued)) if self.lost_too_long() => {
                    if let Some(reply) = queued.reply {
                        drop(reply.send(Err(Error::Text(reconnecting()))));
                    }
                    Ok(())
                }
                Some(Request::Control(queued)) => self.queue(queued),
                Some(request @ (Request::Canvas { .. } | Request::Document { .. })) if self.lost_too_long() => {
                    reject(request, Error::Text(reconnecting()));
                    Ok(())
                }
                Some(request @ (Request::Canvas { .. } | Request::Document { .. })) if interrupted => {
                    self.hold(request);
                    Ok(())
                }
                Some(request @ (Request::Canvas { .. } | Request::Document { .. })) => {
                    self.forward(request);
                    Ok(())
                }
                Some(Request::Picture { picture, reply }) => {
                    drop(reply.send(self.wait_picture(picture)));
                    Ok(())
                }
                None => return Step::Stop(text!("Disconnected from the remote terminal.")),
                }
            }
            _ = tick.tick() => self.maintain().await,
        };
        Step::from(outcome)
    }

    async fn run(mut self) {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut ended = false;
        let reason = loop {
            if self.cancellation.is_cancelled() {
                break text!("Disconnected from the remote terminal.");
            }
            match self.step(&mut tick).await {
                Step::Next => {}
                Step::Failed(failure) => {
                    if let Some(reason) = self.interrupt(failure).await {
                        break reason;
                    }
                }
                Step::Stop(reason) => break reason,
                Step::Ended => {
                    ended = true;
                    break text!("The host stopped this terminal.");
                }
            }
            if self.link == Link::Lost && self.subscribers.is_empty() {
                break text!("Disconnected from the remote terminal.");
            }
        };
        self.cancellation.cancel();
        self.connection.disconnect();
        self.requests.close();
        self.subscribers
            .close(&reason, self.next_sequence.unwrap_or(self.delivered));
        if let Some(sending) = self.sending.take() {
            self.tell_picture(sending.total, PictureState::Failed, 0, Some(reason.clone()))
                .await;
        }
        for waiting in self.pending {
            waiting.reply.reject(Error::Text(reason.clone()));
        }
        for queued in self.queued {
            if let Some(reply) = queued.reply {
                drop(reply.send(Err(Error::Text(reason.clone()))));
            }
        }
        for reply in self.in_flight.into_iter().flat_map(|active| active.replies) {
            drop(reply.send(Err(Error::Text(reason.clone()))));
        }
        for request in self.held {
            reject(request, Error::Text(reason.clone()));
        }
        if ended {
            drop(
                self.changes
                    .send(SessionChange::RemoteEnded {
                        id: self.connection.session.id,
                        instance_id: self.instance_id,
                    })
                    .await,
            );
        }
        drop(
            self.changes
                .send(SessionChange::RemoteClosed {
                    id: self.connection.session.id,
                    incarnation: self.connection.session.incarnation_id,
                    instance_id: self.instance_id,
                    reason,
                })
                .await,
        );
    }
}

fn validate_resize(
    rows: u16,
    cols: u16,
    width: u32,
    height: u32,
    cell_width: u32,
    cell_height: u32,
) -> Result<()> {
    let size = TerminalSize::new(rows, cols).map_err(|error| Error::Invalid(error.to_string()))?;
    if [width, height, cell_width, cell_height] == [0; 4] {
        return Ok(());
    }
    let geometry = TerminalPixelGeometry::new(width, height, cell_width, cell_height)
        .and_then(|geometry| geometry.validate_for_size(size))
        .map_err(|error| Error::Invalid(error.to_string()))?;
    u16::try_from(geometry.width_pixels())
        .map_err(|_| Error::Invalid("terminal pixel width exceeds the PTY limit".to_owned()))?;
    u16::try_from(geometry.height_pixels())
        .map_err(|_| Error::Invalid("terminal pixel height exceeds the PTY limit".to_owned()))?;
    Ok(())
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
