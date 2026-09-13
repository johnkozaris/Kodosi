use std::{
    collections::{HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
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
    Checkpoint, ControlFrame, DataFrame, SessionChange, Subscription, TerminalHistoryPolicy,
    TerminalPixelGeometry, TerminalSize,
};
use crate::{
    Error, Result,
    network::{CheckpointCut, RemoteConnection, RemoteControl, RemoteUpdate, TerminalControl},
};

const CONTROL_CAPACITY: usize = 64;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const INPUT_BUDGET: usize = 4 * 1024 * 1024;
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone)]
pub(crate) struct RemoteTerminal {
    requests: mpsc::Sender<Request>,
    cancellation: CancellationToken,
    input_budget: Arc<Semaphore>,
    pub incarnation_id: Uuid,
    pub instance_id: Uuid,
}

enum Request {
    Subscribe(oneshot::Sender<Result<Subscription>>),
    Checkpoint {
        connection: Uuid,
        reply: oneshot::Sender<Result<CheckpointCut>>,
    },
    Unsubscribe(Uuid),
    Control(QueuedControl),
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
        let terminal = Self {
            requests,
            cancellation: cancellation.clone(),
            input_budget,
            incarnation_id,
            instance_id,
        };
        tokio::spawn(
            RemoteActor::new(connection, receiver, cancellation, changes, instance_id).run(),
        );
        terminal
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.cancellation.is_cancelled() || self.requests.is_closed()
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

    pub(crate) fn admit_input(
        &self,
        connection: Uuid,
        bytes: Bytes,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        let (reply, result) = oneshot::channel();
        let admission = self.queue_input(connection, bytes, Some(reply));
        async move {
            wait_for(admission, result, CONTROL_TIMEOUT)
                .await
                .map(|_| ())
        }
    }

    #[cfg(test)]
    pub(crate) fn input(&self, connection: Uuid, bytes: Bytes) -> Result<()> {
        self.queue_input(connection, bytes, None)
    }

    fn queue_input(
        &self,
        connection: Uuid,
        bytes: Bytes,
        reply: Option<oneshot::Sender<Result<Value>>>,
    ) -> Result<()> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(Error::Invalid("terminal input exceeds 1 MiB".to_owned()));
        }
        let count = u32::try_from(bytes.len()).map_err(|_| Error::Busy)?;
        let budget = Arc::clone(&self.input_budget)
            .try_acquire_many_owned(count)
            .map_err(|_| Error::Busy)?;
        self.send(Request::Control(QueuedControl {
            connection: Some(connection),
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
            Error::Other(
                "The remote terminal did not confirm the request. It was not retried.".to_owned(),
            )
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

struct InFlight {
    result: Pin<Box<dyn Future<Output = crate::network::Result<Value>> + Send>>,
    reply: Option<oneshot::Sender<Result<Value>>>,
    input: bool,
    focus: Option<bool>,
    resize_owner: Option<(Uuid, CancellationToken)>,
    _budget: Option<OwnedSemaphorePermit>,
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
    next_sequence: Option<u64>,
    recent: VecDeque<DataFrame>,
    size: Option<TerminalSize>,
    last_resize: Option<u64>,
    future_checkpoint: Option<CheckpointCut>,
    queued: VecDeque<QueuedControl>,
    in_flight: Option<InFlight>,
    focused: HashSet<Uuid>,
    confirmed_focus: bool,
    resize_owner: Option<(Uuid, CancellationToken)>,
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
            next_sequence: None,
            recent: VecDeque::new(),
            size: None,
            last_resize: None,
            future_checkpoint: None,
            queued: VecDeque::new(),
            in_flight: None,
            focused: HashSet::new(),
            confirmed_focus: false,
            resize_owner: None,
        }
    }

    async fn request_capture(&mut self) -> Result<()> {
        if !self.capture_requested && self.future_checkpoint.is_none() && !self.pending.is_empty() {
            self.controls.request_checkpoint().await?;
            self.capture_requested = true;
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

    fn geometry(&mut self, checkpoint: &Checkpoint, cut: u64) {
        let size = checkpoint.size();
        if self.size.is_some_and(|previous| previous != size) {
            self.subscribers.control(&ControlFrame::Resize {
                rows: size.rows(),
                cols: size.cols(),
                at_sequence: cut,
            });
            self.last_resize = Some(cut);
        }
        self.size = Some(size);
    }

    fn fulfill(&mut self, checkpoint: &Checkpoint, cut: u64) {
        let current = self.next_sequence.unwrap_or(cut);
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
                    self.geometry(&cut.checkpoint, next);
                    self.fulfill(&cut.checkpoint, next);
                }
            }
            RemoteUpdate::Checkpoint {
                checkpoint,
                next_sequence: cut,
                fresh,
            } => {
                if !fresh && self.next_sequence.is_none() {
                    return Ok(());
                }
                validate_terminal_checkpoint(&checkpoint, TerminalHistoryPolicy::default())?;
                if !fresh {
                    let current = self.next_sequence.ok_or(Error::Stale)?;
                    if cut > current {
                        return Err(Error::Other(
                            "Remote resize arrived ahead of its output.".to_owned(),
                        ));
                    }
                    if cut == current {
                        self.geometry(&checkpoint, cut);
                    }
                    return Ok(());
                }
                if let Some(metadata) = checkpoint.metadata.clone() {
                    self.metadata = Some(metadata);
                }
                self.capture_requested = false;
                match self.next_sequence {
                    None => {
                        self.next_sequence = Some(cut);
                        self.geometry(&checkpoint, cut);
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
                    Some(current) if cut == current => self.geometry(&checkpoint, cut),
                    Some(_) => {}
                }
                self.fulfill(&checkpoint, cut);
            }
            RemoteUpdate::Ended { .. } => return Err(Error::Stopped),
            RemoteUpdate::Closed { reason } => return Err(Error::Other(reason)),
        }
        Ok(())
    }

    fn queue(&mut self, queued: QueuedControl) -> Result<()> {
        if self.queued.len() >= CONTROL_CAPACITY {
            if let Some(reply) = queued.reply {
                drop(reply.send(Err(Error::Busy)));
                return Ok(());
            }
            return Err(Error::Other(
                "Remote input exceeded its connection buffer. It was not retried.".to_owned(),
            ));
        }
        self.queued.push_back(queued);
        Ok(())
    }

    fn next_control(&mut self) -> Result<()> {
        if self.in_flight.is_some() {
            return Ok(());
        }
        self.focused.retain(|id| self.subscribers.contains(*id));
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|(_, token)| token.is_cancelled())
        {
            self.resize_owner = None;
        }
        let aggregate = !self.focused.is_empty();
        if aggregate != self.confirmed_focus {
            self.begin(
                TerminalControl::Focus { focused: aggregate },
                None,
                None,
                None,
            );
            return Ok(());
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
                    let id = match self.check_resize(queued.connection, &control) {
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
                TerminalControl::Input { .. } => {
                    if authorization.is_none() {
                        return Err(Error::Stale);
                    }
                }
                TerminalControl::Interrupt | TerminalControl::Stop => {}
            }
            self.begin(control, queued.reply, resize_owner, queued.budget);
            break;
        }
        Ok(())
    }

    fn check_resize(&self, connection: Option<Uuid>, control: &TerminalControl) -> Result<Uuid> {
        let id = connection.ok_or(Error::Stale)?;
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
        if self
            .resize_owner
            .as_ref()
            .is_some_and(|(owner, token)| *owner != id && !token.is_cancelled())
            && !claim
        {
            return Err(Error::Invalid(
                "Another terminal view controls the size. Focus this view to resize it.".to_owned(),
            ));
        }
        Ok(id)
    }

    fn begin(
        &mut self,
        control: TerminalControl,
        reply: Option<oneshot::Sender<Result<Value>>>,
        resize_owner: Option<(Uuid, CancellationToken)>,
        budget: Option<OwnedSemaphorePermit>,
    ) {
        let focus = match &control {
            TerminalControl::Focus { focused } => Some(*focused),
            _ => None,
        };
        let input = matches!(control, TerminalControl::Input { .. });
        let controls = self.controls.clone();
        self.in_flight = Some(InFlight {
            result: Box::pin(async move { controls.send_control(control).await }),
            reply,
            input,
            focus,
            resize_owner,
            _budget: budget,
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
                format!("Remote control was not confirmed: {error}. It was not retried.")
            });
        }
        if let Some(reply) = active.reply {
            drop(reply.send(result.map_err(Error::from)));
        }
        fatal.map_or(Ok(()), |reason| Err(Error::Other(reason)))
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
        if self.pending.is_empty() {
            self.capture_requested = false;
        }
        self.request_capture().await
    }

    async fn run(mut self) {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut ended = false;
        let reason = loop {
            if self.cancellation.is_cancelled() {
                break "Disconnected from the remote terminal.".to_owned();
            }
            if let Err(error) = self.next_control() {
                break error.to_string();
            }
            tokio::select! {
                () = self.cancellation.cancelled() => break "Disconnected from the remote terminal.".to_owned(),
                result = async { match self.in_flight.as_mut() { Some(active) => active.result.as_mut().await, None => std::future::pending().await } } => {
                    if let Err(error) = self.finish_control(result) { break error.to_string(); }
                }
                update = self.connection.updates.recv() => {
                    if matches!(&update, None | Some(RemoteUpdate::Closed { .. } | RemoteUpdate::Ended { .. }))
                        && let Some(result) = self.in_flight.as_mut().and_then(|active| active.result.as_mut().now_or_never())
                        && let Err(error) = self.finish_control(result)
                    {
                        break error.to_string();
                    }
                    match update {
                        Some(RemoteUpdate::Ended { final_sequence }) => {
                            if self.next_sequence != Some(final_sequence) { break "Terminal end crossed output boundary.".to_owned(); }
                            ended = true;
                            break "The host stopped this terminal.".to_owned();
                        }
                        Some(update) => { if let Err(error) = self.update(update) { break error.to_string(); } }
                        None => break "The remote host disconnected.".to_owned(),
                    }
                },
                request = self.requests.recv() => match request {
                    Some(Request::Subscribe(reply)) => { if let Err(error) = self.add_capture(CaptureReply::Subscribe(reply)).await { break error.to_string(); } }
                    Some(Request::Checkpoint { connection, reply }) => { if let Err(error) = self.add_capture(CaptureReply::Checkpoint { connection, reply }).await { break error.to_string(); } }
                    Some(Request::Unsubscribe(id)) => { self.subscribers.remove(id); self.focused.remove(&id); }
                    Some(Request::Control(queued)) => { if let Err(error) = self.queue(queued) { break error.to_string(); } }
                    None => break "Disconnected from the remote terminal.".to_owned(),
                },
                _ = tick.tick() => { if let Err(error) = self.maintain().await { break error.to_string(); } }
            }
        };
        self.cancellation.cancel();
        self.connection.disconnect();
        self.requests.close();
        self.subscribers
            .close(&reason, self.next_sequence.unwrap_or(0));
        for waiting in self.pending {
            waiting.reply.reject(Error::Other(reason.clone()));
        }
        for queued in self.queued {
            if let Some(reply) = queued.reply {
                drop(reply.send(Err(Error::Other(reason.clone()))));
            }
        }
        if let Some(active) = self.in_flight
            && let Some(reply) = active.reply
        {
            drop(reply.send(Err(Error::Other(reason.clone()))));
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
