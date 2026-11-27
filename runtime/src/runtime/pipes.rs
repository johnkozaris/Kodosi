use std::{collections::VecDeque, time::Duration};

use base64::Engine as _;
use bytes::Bytes;
use serde_json::{Value, json};
use stream::viewer;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Completion, Runtime, Scope};
use crate::{
    Command, Error, Result,
    canvas::PipeEnds,
    protocol::{SessionStatus, parse_id},
    terminal::SessionChange,
    text::Text,
    video::{Arrival, SHOWN_QUEUE, Shown, TOOK_QUEUE, Took, VideoWatch, Viewing},
};

const MOST_PIPES: usize = 32;
/// Acknowledgements and asks that wait for room on a video pipe; while it is full, the viewer
/// takes no more frames.
const MOST_WAITING_ACKS: usize = 64;
/// Grace period before reporting a disconnected pipe to the client.
const PATIENCE: Duration = Duration::from_millis(1500);
const LONGEST_WAIT: Duration = Duration::from_secs(30);

/// Client-owned document or browser pipe managed by the runtime. Reopens immediately when its
/// terminal comes back online, and backs off exponentially when opens fail while online.
pub(super) struct Kept {
    session: Uuid,
    key: String,
    consumer: Uuid,
    lifetime: CancellationToken,
    news: mpsc::UnboundedSender<News>,
    state: State,
    failures: u32,
    /// True while the client is mid-frame (`more == true`).
    within_frame: bool,
    opened: bool,
    told: Told,
}

#[derive(PartialEq, Eq)]
enum Told {
    Nothing,
    Down(Option<Text>),
}

enum State {
    Waiting {
        at: Instant,
        for_terminal: bool,
    },
    Opening(Uuid),
    Open {
        attempt: Uuid,
        sender: mpsc::Sender<Vec<u8>>,
        filling: Vec<u8>,
        /// Discards trailing chunks of a client frame that began before this open completed.
        skipping: bool,
    },
}

enum News {
    Open {
        attempt: Uuid,
        frames: mpsc::Receiver<Vec<u8>>,
        /// The attempt's way to the stream, for a video pipe's acknowledgements.
        sender: mpsc::Sender<Vec<u8>>,
        scope: Scope,
    },
    /// The app watches the video of the pipe from now on.
    Watch(Outlet),
    Down {
        reason: Option<Text>,
        scope: Scope,
    },
    Closed {
        scope: Scope,
    },
}

impl Kept {
    fn tell(&self, news: News) {
        drop(self.news.send(news));
    }

    fn wait(&mut self, failed: bool) {
        self.failures = if failed {
            self.failures.saturating_add(1)
        } else {
            0
        };
        self.state = State::Waiting {
            at: Instant::now() + retry_wait(self.failures),
            for_terminal: false,
        };
    }

    /// Suppresses bare disconnect notices before the first open; only explicit errors are reported.
    fn fall(&mut self, reason: Option<Text>, scope: &Scope) {
        if reason.is_none() && !self.opened {
            return;
        }
        let told = Told::Down(reason.clone());
        if self.told != told {
            self.told = told;
            self.tell(News::Down {
                reason,
                scope: scope.clone(),
            });
        }
    }

    fn waits_for(&self, attempt: Uuid) -> bool {
        matches!(self.state, State::Opening(opening) if opening == attempt)
    }

    fn carries(&self, attempt: Uuid) -> bool {
        matches!(self.state, State::Open { attempt: open, .. } if open == attempt)
    }
}

/// The shared browser and the screen; their apps watch them as video.
fn shows_video(key: &str) -> bool {
    key == crate::canvas::BROWSER_PIPE || key == crate::canvas::SCREEN_PIPE
}

fn retry_wait(failures: u32) -> Duration {
    match failures {
        0 => Duration::ZERO,
        failures => Duration::from_secs(1)
            .saturating_mul(1 << (failures - 1).min(5))
            .min(LONGEST_WAIT),
    }
}

impl Runtime {
    pub(super) fn open_document(
        &mut self,
        command: &Command,
        consumer: Uuid,
        lifetime: CancellationToken,
    ) -> Result<()> {
        let Command::DocumentOpen {
            session_id,
            pipe,
            key,
            ..
        } = command
        else {
            return Err(Error::Invalid("expected a document".to_owned()));
        };
        let session = parse_id(session_id)?;
        let pipe = parse_id(pipe)?;
        if !self.listed(&session) {
            return Err(Error::NotFound);
        }
        self.prune_pipes();
        if self.pipes.contains_key(&pipe) {
            return Err(Error::Invalid("The pipe is already open.".to_owned()));
        }
        if self.pipes.len() >= MOST_PIPES {
            tracing::warn!(
                pipes = MOST_PIPES,
                "the host keeps the most documents open, so a view waits"
            );
            return Err(Error::Busy);
        }
        let (news, heard) = mpsc::unbounded_channel();
        let carrier = Carrier {
            events: self.events.clone(),
            direct: self
                .direct
                .get(&consumer)
                .filter(|direct| !direct.is_closed())
                .cloned(),
            consumer,
        };
        tokio::spawn(
            Delivery {
                pipe,
                carrier,
                scope: self.scope.clone(),
                news: heard,
                ended: self.changes.clone(),
                frames: None,
                carried: false,
                sequence: 0,
                lost_at: None,
                reason: None,
                told: false,
                video: shows_video(key).then(Video::new),
            }
            .run(),
        );
        self.pipes.insert(
            pipe,
            Kept {
                session,
                key: key.clone(),
                consumer,
                lifetime,
                news,
                state: State::Waiting {
                    at: Instant::now(),
                    for_terminal: false,
                },
                failures: 0,
                within_frame: false,
                opened: false,
                told: Told::Nothing,
            },
        );
        self.result(command);
        self.keep_pipes();
        Ok(())
    }

    pub(super) fn close_document(&mut self, command: &Command) -> Result<()> {
        let Command::DocumentClose { pipe, .. } = command else {
            return Err(Error::Invalid("expected a document".to_owned()));
        };
        self.pipes.remove(&parse_id(pipe)?);
        Ok(())
    }

    pub(super) fn send_to_document(&mut self, command: &Command) -> Result<()> {
        let Command::DocumentSend {
            pipe, more, data, ..
        } = command
        else {
            return Err(Error::Invalid("expected a document frame".to_owned()));
        };
        let id = parse_id(pipe)?;
        let part = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| Error::Invalid("The document part is not valid.".to_owned()))?;
        let scope = self.scope.clone();
        let kept = self.pipes.get_mut(&id).ok_or(Error::NotFound)?;
        kept.within_frame = *more;
        let State::Open {
            sender,
            filling,
            skipping,
            ..
        } = &mut kept.state
        else {
            return Ok(());
        };
        if *skipping {
            *skipping = *more;
            return Ok(());
        }
        let failure = if filling.len() + part.len() > crate::canvas::MOST_PIPE_FRAME {
            Some(Error::Invalid("The document frame is too long.".to_owned()))
        } else {
            filling.extend_from_slice(&part);
            (!*more && sender.try_send(std::mem::take(filling)).is_err()).then_some(Error::Busy)
        };
        let Some(failure) = failure else {
            return Ok(());
        };
        kept.wait(true);
        kept.fall(None, &scope);
        Err(failure)
    }

    pub(super) fn watch_video(&mut self, pipe: Uuid, reply: oneshot::Sender<Result<VideoWatch>>) {
        self.prune_pipes();
        let watch = match self.pipes.get(&pipe) {
            None => Err(Error::NotFound),
            Some(kept) if !shows_video(&kept.key) => {
                Err(Error::Invalid("The pipe shows no video.".to_owned()))
            }
            Some(kept) => {
                let (shown, shown_here) = mpsc::channel(SHOWN_QUEUE);
                let (took_here, took) = mpsc::channel(TOOK_QUEUE);
                kept.tell(News::Watch(Outlet { shown, took }));
                Ok(VideoWatch {
                    shown: shown_here,
                    took: took_here,
                })
            }
        };
        drop(reply.send(watch));
    }

    pub(super) fn prune_pipes(&mut self) {
        self.pipes.retain(|_, kept| !kept.lifetime.is_cancelled());
    }

    /// Resets all pipes to reopen under the new account; pipes for unlisted terminals close on the
    /// next `keep_pipes()` pass.
    pub(super) fn reopen_pipes(&mut self) {
        let scope = self.scope.clone();
        for kept in self.pipes.values_mut() {
            kept.wait(false);
            kept.told = Told::Nothing;
            kept.fall(None, &scope);
        }
    }

    pub(super) fn keep_pipes(&mut self) {
        self.prune_pipes();
        let scope = self.scope.clone();
        let now = Instant::now();
        let pipes = self.pipes.keys().copied().collect::<Vec<_>>();
        for pipe in pipes {
            let Some(session) = self.pipes.get(&pipe).map(|kept| kept.session) else {
                continue;
            };
            if !self.listed(&session) {
                if let Some(kept) = self.pipes.remove(&pipe) {
                    kept.tell(News::Closed {
                        scope: scope.clone(),
                    });
                }
                continue;
            }
            let runs = self.runs(session);
            let reason =
                (!runs && !self.host_online(session)).then(|| self.offline(session).text());
            let Some(kept) = self.pipes.get_mut(&pipe) else {
                continue;
            };
            let State::Waiting { at, for_terminal } = kept.state else {
                continue;
            };
            if !runs {
                kept.state = State::Waiting {
                    at,
                    for_terminal: true,
                };
                kept.fall(reason, &scope);
            } else if for_terminal || at <= now {
                if for_terminal {
                    kept.failures = 0;
                }
                self.open_kept(pipe);
            }
        }
    }

    /// True if the terminal can accept a pipe (opening one wakes a dormant remote terminal).
    fn runs(&self, id: Uuid) -> bool {
        self.target(id).is_ok()
            && (self.hosts(&id)
                || self.remote_status(id, self.host_online(id)) == SessionStatus::Running)
    }

    fn open_kept(&mut self, pipe: Uuid) {
        let Some(kept) = self.pipes.get(&pipe) else {
            return;
        };
        let opening = self
            .job_capacity()
            .and_then(|()| self.target(kept.session))
            .map(|target| {
                let attempt = Uuid::now_v7();
                // Only the owner's devices watch the screen, so the computer names the device.
                let name = if kept.key == crate::canvas::SCREEN_PIPE {
                    crate::network::host_label()
                } else {
                    self.person.clone().unwrap_or_else(|| "Someone".to_owned())
                };
                let opening = target.document(
                    attempt,
                    kept.key.clone(),
                    name,
                    self.scope.user.clone(),
                    kept.consumer,
                );
                (attempt, opening)
            });
        let Some(kept) = self.pipes.get_mut(&pipe) else {
            return;
        };
        match opening {
            Ok((attempt, opening)) => {
                kept.state = State::Opening(attempt);
                self.spawn(async move {
                    Completion::Document {
                        pipe,
                        attempt,
                        result: opening.await,
                    }
                });
            }
            Err(_) => {
                kept.state = State::Waiting {
                    at: Instant::now() + Duration::from_secs(1),
                    for_terminal: false,
                };
            }
        }
    }

    pub(super) fn document_opened(
        &mut self,
        pipe: Uuid,
        attempt: Uuid,
        result: Result<PipeEnds>,
        current: bool,
    ) {
        let scope = self.scope.clone();
        let Some(kept) = self
            .pipes
            .get_mut(&pipe)
            .filter(|kept| kept.waits_for(attempt))
        else {
            return;
        };
        match result {
            Ok(ends) if current => {
                let sender = ends.sender.clone();
                kept.state = State::Open {
                    attempt,
                    sender: ends.sender,
                    filling: Vec::new(),
                    skipping: kept.within_frame,
                };
                kept.opened = true;
                kept.told = Told::Nothing;
                kept.tell(News::Open {
                    attempt,
                    frames: ends.receiver,
                    sender,
                    scope,
                });
            }
            Ok(_) => kept.wait(false),
            Err(error) => {
                kept.wait(true);
                kept.fall(Some(error.text()), &scope);
            }
        }
    }

    /// Counts a pipe that closes before delivering any frame as a failure so backoff engages if
    /// the host immediately rejects the document.
    pub(super) fn document_ended(&mut self, pipe: Uuid, attempt: Uuid, carried: bool) {
        if let Some(kept) = self
            .pipes
            .get_mut(&pipe)
            .filter(|kept| kept.carries(attempt))
        {
            kept.wait(!carried);
        }
    }

    pub(super) fn pipes_due(&self) -> Option<Instant> {
        self.pipes
            .values()
            .filter_map(|kept| match kept.state {
                State::Waiting {
                    at,
                    for_terminal: false,
                } => Some(at),
                _ => None,
            })
            .min()
    }
}

struct Carrier {
    events: broadcast::Sender<crate::Event>,
    direct: Option<mpsc::Sender<crate::Event>>,
    consumer: Uuid,
}

impl Carrier {
    async fn send(&self, scope: &Scope, value: Value) -> bool {
        let Ok(mut event) = crate::Event::new(scope.user.clone(), scope.epoch, value) else {
            return true;
        };
        event.recipient = Some(self.consumer);
        if let Some(direct) = &self.direct {
            return direct.send(event).await.is_ok();
        }
        drop(self.events.send(event));
        true
    }
}

/// The app's end of a watched video pipe, as the runtime holds it.
struct Outlet {
    shown: mpsc::Sender<Shown>,
    took: mpsc::Receiver<Took>,
}

/// The viewer's side of a video pipe: it hands the app only frames that its decoder can take,
/// acknowledges each frame that arrives, and asks for keyframes.
struct Video {
    viewing: Viewing,
    outlet: Option<Outlet>,
    /// The current attempt's way to the stream.
    stream: Option<mpsc::Sender<Vec<u8>>>,
    /// Acknowledgements and asks that wait for room on `stream`, in order. The host's flow
    /// control counts a frame without an acknowledgement as lost, so none is dropped.
    waiting: VecDeque<Vec<u8>>,
    at_app: Option<Handed>,
    /// A message that came before the app watched, so that the pipe still notices its end.
    held: Option<Bytes>,
}

/// The frame that the app's decoder has, until the app answers.
struct Handed {
    id: u32,
    attempt: Option<Uuid>,
    /// False once the decoder started over, so the answer acknowledges the frame and no more.
    decides: bool,
}

impl Video {
    const fn new() -> Self {
        Self {
            viewing: Viewing::new(),
            outlet: None,
            stream: None,
            waiting: VecDeque::new(),
            at_app: None,
            held: None,
        }
    }

    fn takes(&self) -> bool {
        self.held.is_none() && self.at_app.is_none() && self.waiting.len() < MOST_WAITING_ACKS
    }

    fn opened(&mut self, stream: Option<mpsc::Sender<Vec<u8>>>) {
        self.viewing.opened(Instant::now());
        self.stream = stream;
        self.waiting.clear();
        self.held = None;
    }

    async fn carry(&mut self, message: Bytes, attempt: Option<Uuid>) {
        if self.outlet.is_none() {
            self.held = Some(message);
            return;
        }
        match message.first() {
            Some(&viewer::JSON) => {
                self.show(Shown::Message(message)).await;
            }
            Some(&viewer::FRAME) => self.frame(message, attempt).await,
            Some(&viewer::UNCHANGED) => self.unchanged().await,
            _ => {}
        }
    }

    fn watched(&mut self, outlet: Outlet) {
        self.at_app = None;
        if self.outlet.replace(outlet).is_some() {
            self.viewing.failed(Instant::now());
            self.ask();
        }
    }

    async fn show(&mut self, shown: Shown) -> bool {
        let Some(outlet) = &self.outlet else {
            return false;
        };
        let sent = outlet.shown.send(shown).await.is_ok();
        if !sent {
            self.outlet = None;
        }
        sent
    }

    async fn frame(&mut self, message: Bytes, attempt: Option<Uuid>) {
        let Some(frame) = viewer::Frame::decode(&message) else {
            return;
        };
        match self.viewing.arrived(frame.id, frame.kind, Instant::now()) {
            Arrival::Hand => {
                if self.show(Shown::Frame(message)).await {
                    self.at_app = Some(Handed {
                        id: frame.id,
                        attempt,
                        decides: true,
                    });
                } else {
                    self.viewing.missed(Instant::now());
                }
            }
            Arrival::Drop { ask } => {
                self.acknowledge(frame.id);
                if ask {
                    self.ask();
                }
            }
        }
    }

    async fn unchanged(&mut self) {
        if self.viewing.unchanged(Instant::now()) {
            self.show(Shown::Stale(false)).await;
        }
    }

    fn took(&mut self, took: Option<Took>, attempt: Option<Uuid>) {
        let now = Instant::now();
        match took {
            None => {
                self.outlet = None;
                if self.at_app.take().is_some() {
                    self.viewing.missed(now);
                }
            }
            Some(Took::Restart) => {
                if let Some(handed) = &mut self.at_app {
                    handed.decides = false;
                }
                self.viewing.failed(now);
                self.ask();
            }
            Some(answer) => {
                let Some(handed) = self
                    .at_app
                    .take()
                    .filter(|handed| handed.attempt == attempt)
                else {
                    return;
                };
                self.acknowledge(handed.id);
                if !handed.decides {
                    return;
                }
                if answer == Took::Taken {
                    self.viewing.taken(now);
                } else {
                    self.viewing.failed(now);
                    self.ask();
                }
            }
        }
    }

    /// While the app has a frame the viewer reads nothing more, so no wait or silence counts.
    fn stale_due(&self) -> Option<Instant> {
        (self.outlet.is_some() && self.at_app.is_none())
            .then(|| self.viewing.stale_due())
            .flatten()
    }

    fn acknowledge(&mut self, frame: u32) {
        self.say(&json!({ "type": "ack", "frame": frame }));
    }

    fn ask(&mut self) {
        self.say(&json!({ "type": "keyframe" }));
    }

    fn say(&mut self, message: &Value) {
        let Some(stream) = &self.stream else {
            return;
        };
        let message = viewer::json(message).to_vec();
        if !self.waiting.is_empty() {
            self.waiting.push_back(message);
            return;
        }
        if let Err(mpsc::error::TrySendError::Full(message)) = stream.try_send(message) {
            self.waiting.push_back(message);
        }
    }

    fn wants_room(&self) -> Option<mpsc::Sender<Vec<u8>>> {
        self.stream.clone().filter(|_| !self.waiting.is_empty())
    }

    fn room(&mut self, permit: Option<mpsc::OwnedPermit<Vec<u8>>>) {
        let (Some(permit), Some(message)) = (permit, self.waiting.pop_front()) else {
            self.waiting.clear();
            return;
        };
        permit.send(message);
        while let (Some(stream), Some(message)) = (&self.stream, self.waiting.pop_front()) {
            if let Err(mpsc::error::TrySendError::Full(message)) = stream.try_send(message) {
                self.waiting.push_front(message);
                return;
            }
        }
    }
}

/// Serializes `document.opened`, `document.frame`, and `document.lost` per pipe so the client
/// sees lifecycle events and frames in strict order. A video pipe's frames and messages go to the
/// app that watches it instead, in pipe order too.
struct Delivery {
    pipe: Uuid,
    carrier: Carrier,
    scope: Scope,
    news: mpsc::UnboundedReceiver<News>,
    ended: mpsc::Sender<SessionChange>,
    frames: Option<(Uuid, mpsc::Receiver<Vec<u8>>)>,
    carried: bool,
    sequence: u64,
    lost_at: Option<Instant>,
    reason: Option<Text>,
    told: bool,
    video: Option<Video>,
}

impl Delivery {
    async fn run(mut self) {
        loop {
            let patience = self.lost_at.filter(|_| !self.told).map(|at| at + PATIENCE);
            let takes = self.video.as_ref().is_none_or(Video::takes);
            let room = self.video.as_ref().and_then(Video::wants_room);
            let stale = self.video.as_ref().and_then(Video::stale_due);
            let going = tokio::select! {
                news = self.news.recv() => match news {
                    Some(news) => self.heard(news).await,
                    None => false,
                },
                frame = next_frame(&mut self.frames), if takes => match frame {
                    Some(frame) => self.carry(frame).await,
                    None => self.lost().await,
                },
                took = next_took(&mut self.video) => {
                    let attempt = self.attempt();
                    if let Some(video) = &mut self.video {
                        video.took(took, attempt);
                    }
                    true
                }
                permit = room_on(room) => {
                    if let Some(video) = &mut self.video {
                        video.room(permit);
                    }
                    true
                }
                () = until(stale) => {
                    if let Some(video) = &mut self.video {
                        video.viewing.went_stale();
                        video.show(Shown::Stale(true)).await;
                    }
                    true
                }
                () = tokio::time::sleep_until(patience.unwrap_or_else(Instant::now)), if patience.is_some() => {
                    self.tell_lost().await
                }
            };
            if !going {
                return;
            }
        }
    }

    fn attempt(&self) -> Option<Uuid> {
        self.frames.as_ref().map(|(attempt, _)| *attempt)
    }

    async fn heard(&mut self, news: News) -> bool {
        match news {
            News::Open {
                attempt,
                frames,
                sender,
                scope,
            } => {
                self.scope = scope;
                self.frames = Some((attempt, frames));
                self.carried = false;
                self.sequence = 0;
                if let Some(video) = &mut self.video {
                    video.opened(Some(sender));
                }
                self.send(json!({ "type": "document.opened", "pipe": self.pipe }))
                    .await
            }
            News::Watch(outlet) => {
                let attempt = self.attempt();
                if let Some(video) = &mut self.video {
                    video.watched(outlet);
                    if let Some(held) = video.held.take() {
                        video.carry(held, attempt).await;
                    }
                }
                true
            }
            News::Down { reason, scope } => {
                self.scope = scope;
                self.frames = None;
                if let Some(video) = &mut self.video {
                    video.opened(None);
                }
                self.lost_at.get_or_insert_with(Instant::now);
                let changed = self.reason != reason;
                self.reason = reason;
                !(self.told && changed) || self.tell_lost().await
            }
            News::Closed { scope } => {
                self.scope = scope;
                self.send(json!({ "type": "document.closed", "pipe": self.pipe }))
                    .await;
                false
            }
        }
    }

    async fn carry(&mut self, frame: Vec<u8>) -> bool {
        if !self.carried {
            self.carried = true;
            self.lost_at = None;
            self.reason = None;
            self.told = false;
        }
        let attempt = self.attempt();
        if let Some(video) = &mut self.video {
            video.carry(Bytes::from(frame), attempt).await;
            return true;
        }
        let parts = frame.len().div_ceil(crate::canvas::PART_BYTES);
        for (index, part) in frame.chunks(crate::canvas::PART_BYTES).enumerate() {
            let sent = self
                .send(json!({
                    "type": "document.frame",
                    "pipe": self.pipe,
                    "sequence": self.sequence,
                    "more": index + 1 < parts,
                    "data": base64::engine::general_purpose::STANDARD.encode(part),
                }))
                .await;
            if !sent {
                return false;
            }
            self.sequence += 1;
        }
        true
    }

    async fn lost(&mut self) -> bool {
        let Some((attempt, _)) = self.frames.take() else {
            return true;
        };
        if let Some(video) = &mut self.video {
            video.opened(None);
        }
        self.lost_at.get_or_insert_with(Instant::now);
        self.ended
            .send(SessionChange::PipeEnded {
                pipe: self.pipe,
                attempt,
                carried: self.carried,
            })
            .await
            .is_ok()
    }

    async fn tell_lost(&mut self) -> bool {
        self.told = true;
        self.send(json!({
            "type": "document.lost",
            "pipe": self.pipe,
            "message": self.reason,
        }))
        .await
    }

    async fn send(&self, value: Value) -> bool {
        self.carrier.send(&self.scope, value).await
    }
}

async fn next_frame(frames: &mut Option<(Uuid, mpsc::Receiver<Vec<u8>>)>) -> Option<Vec<u8>> {
    match frames {
        Some((_, frames)) => frames.recv().await,
        None => std::future::pending().await,
    }
}

async fn next_took(video: &mut Option<Video>) -> Option<Took> {
    match video.as_mut().and_then(|video| video.outlet.as_mut()) {
        Some(outlet) => outlet.took.recv().await,
        None => std::future::pending().await,
    }
}

async fn room_on(stream: Option<mpsc::Sender<Vec<u8>>>) -> Option<mpsc::OwnedPermit<Vec<u8>>> {
    match stream {
        Some(stream) => stream.reserve_owned().await.ok(),
        None => std::future::pending().await,
    }
}

async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
