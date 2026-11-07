use std::collections::{HashMap, VecDeque};

use futures_util::stream::{FuturesOrdered, FuturesUnordered};
use tokio::task::JoinSet;

use super::{
    channel::{Channel, Transport},
    direct::Door,
    link::{HostEvent, Hosted, Pipe},
    pacer::Pacer,
    wire::{Accept, ControlResult, End, Frame, PictureOffer, PictureState, PictureStep, Refuse},
    *,
};
use crate::{
    canvas::{MOST_PIPE_FRAME, PIPE_QUEUE},
    network::{CheckpointCut, HostPicture, TerminalStatus},
    terminal::{Picture, TERMINAL_PICTURE_MAX_BYTES, TerminalMetadata},
    text::Text,
};

const VIEWERS: usize = crate::protocol::TERMINAL_VIEWERS_MOST;
const HEARTBEAT: Duration = Duration::from_secs(15);
const LATE_HEARTBEATS: u32 = 2;
const KEYFRAME_SPACING: Duration = Duration::from_millis(500);
const EXACT_AFTER_QUIET: Duration = Duration::from_secs(1);
const UNCONFIRMED_INPUT: u64 = 256 * 1024;
const STATUS_SPACING: Duration = Duration::from_secs(1);

type Reply = oneshot::Receiver<std::result::Result<Value, Text>>;
type PictureAnswer =
    std::pin::Pin<Box<dyn Future<Output = (PictureOffer, std::result::Result<bool, Text>)> + Send>>;

/// The pictures that all views of this host's terminals send at once.
static PICTURE_MEMORY: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(4 * TERMINAL_PICTURE_MAX_BYTES);

/// A picture whose parts a view sends.
struct Receiving {
    offer: PictureOffer,
    bytes: Vec<u8>,
    asked: tokio::time::Instant,
    _memory: tokio::sync::SemaphorePermit<'static>,
}
type Inputs = FuturesOrdered<std::pin::Pin<Box<dyn Future<Output = (u64, bool)> + Send>>>;
type Controls = FuturesUnordered<std::pin::Pin<Box<dyn Future<Output = Frame> + Send>>>;

#[derive(Clone)]
struct Admitted {
    user: String,
    device: String,
    key: Vec<u8>,
    cancel: CancellationToken,
}

type Viewers = Arc<std::sync::Mutex<BTreeMap<Uuid, Admitted>>>;

struct Shared {
    reported: Option<TerminalStatus>,
    held: Option<TerminalStatus>,
    due: tokio::time::Instant,
}

impl Shared {
    fn wanted(&self) -> Option<&TerminalStatus> {
        self.reported
            .as_ref()
            .filter(|status| **status != TerminalStatus::default())
    }

    fn waiting(&self) -> bool {
        self.wanted() != self.held.as_ref()
    }

    fn send(&mut self, hosted: &Hosted) {
        self.held = self.wanted().cloned();
        hosted.share(self.held.clone());
        self.due = tokio::time::Instant::now() + STATUS_SPACING;
    }
}

pub(super) async fn run(
    network: &BackendClient,
    publication: &Arc<Publication>,
    output: &mut PublicationOutput,
    generation: u64,
) -> Result<()> {
    check_generation(network, generation)?;
    let credentials = network.credentials()?;
    let current = current_publication(network, publication, &credentials).await?;
    close_when_asked(network, publication, &credentials).await?;
    let mut hosted = network.inner.link.host(current.id, current.incarnation_id);
    check_generation(network, generation)?;
    drop(
        publication
            .requests
            .send(HostRequest::ResetPresence {
                owner: Some(credentials.user_id.clone()),
            })
            .await,
    );
    let viewers = Viewers::default();
    let stop = credentials.cancel.child_token();
    let _stop = stop.clone().drop_guard();
    let mut tasks = JoinSet::new();
    let mut trust = tokio::time::interval(TRUST_CHECK);
    trust.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    trust.tick().await;
    let mut shared = Shared {
        reported: None,
        held: None,
        due: tokio::time::Instant::now(),
    };
    let result = async {
        loop {
            check_generation(network, generation)?;
            tokio::select! {
                biased;
                () = publication.cancel.cancelled() => return Ok(()),
                () = network.inner.shutdown.cancelled() => return Ok(()),
                () = publication.refresh.notified() => review(network, publication, &network.credentials()?, &viewers).await?,
                _ = trust.tick() => review(network, publication, &network.credentials()?, &viewers).await?,
                Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
                () = tokio::time::sleep_until(shared.due), if shared.waiting() => shared.send(&hosted),
                event = hosted.events.recv() => match event.ok_or(Error::Closed)? {
                    HostEvent::Refused(error) => return Err(error),
                    HostEvent::AccessChanged => review(network, publication, &network.credentials()?, &viewers).await?,
                    HostEvent::Viewer { pipe, user, device } => {
                        if tasks.len() < VIEWERS * 2 {
                            tasks.spawn(viewer(
                                network.clone(),
                                Arc::clone(publication),
                                network.credentials()?,
                                output.resubscribe(),
                                pipe,
                                (user, device),
                                Arc::clone(&viewers),
                                stop.child_token(),
                            ));
                        }
                    }
                },
                frame = output.recv() => match frame {
                    Ok(PublishedFrame::Closed { .. }) | Err(broadcast::error::RecvError::Closed) => {
                        let _drained = tokio::time::timeout(Duration::from_secs(10), async {
                            while tasks.join_next().await.is_some() {}
                        })
                        .await;
                        publication.cancel.cancel();
                        return Ok(());
                    }
                    Ok(PublishedFrame::Metadata(metadata)) => shared.reported = Some(TerminalStatus::of(&metadata)),
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                },
            }
        }
    }
    .await;
    stop.cancel();
    let _ended = tokio::time::timeout(Duration::from_secs(2), async {
        while tasks.join_next().await.is_some() {}
    })
    .await;
    drop(
        publication
            .requests
            .send(HostRequest::ResetPresence { owner: None })
            .await,
    );
    result
}

async fn current_publication(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
) -> Result<SessionDto> {
    let _operation = network.inner.operations.lock().await;
    network.check_credentials(credentials)?;
    let info = publication.info.read().await.clone();
    match network
        .inner
        .http
        .device::<SessionDto>(
            Method::GET,
            &format!("api/sessions/{}", info.session_id),
            credentials,
            None,
        )
        .await
    {
        Ok(dto) => {
            if dto.incarnation_id != info.incarnation_id
                || dto.host_device_id != credentials.keys.device_id
                || dto.owner_user_id != credentials.user_id
            {
                return Err(Error::Stale);
            }
            let local = (info.mission_id, info.room_sequence);
            if let Some((room, sequence)) = network.room_change(credentials, local, &dto).await {
                let mut local = publication.info.write().await;
                local.mission_id = room;
                local.room_sequence = sequence;
                if room.is_some() {
                    local.shared_with = dto.shared_with.iter().cloned().collect();
                }
            }
            *publication.dto.write().await = dto.clone();
            Ok(dto)
        }
        Err(Error::Backend { status: 404, .. }) => {
            network.check_credentials(credentials)?;
            if publication.cancel.is_cancelled() {
                return Err(Error::Stale);
            }
            publication.clear_sharing().await;
            let body = crate::network::backend_client::publication_body(
                &info,
                &credentials.keys.device_id,
                None,
            );
            let dto: SessionDto = network
                .inner
                .http
                .device(Method::POST, "api/sessions", credentials, Some(body))
                .await?;
            *publication.dto.write().await = dto.clone();
            network.emit_for(credentials.generation,Some(credentials.user_id.clone()),json!({"type":"system.error","message":text!("The server no longer had this terminal. Choose friends again.")}));
            Ok(dto)
        }
        Err(error) => Err(error),
    }
}

async fn allowed(publication: &Publication, credentials: &Credentials, user: &str) -> bool {
    if user == credentials.user_id {
        return true;
    }
    let local = publication.info.read().await;
    let published = publication.dto.read().await;
    let chosen_room = local.mission_id.is_some() && local.mission_id == published.mission_id;
    (chosen_room || local.shared_with.contains(user))
        && published.shared_with.iter().any(|member| member == user)
}

async fn review(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
    viewers: &Viewers,
) -> Result<()> {
    let session = publication.info.read().await.session_id;
    match network
        .inner
        .http
        .device::<SessionDto>(
            Method::GET,
            &format!("api/sessions/{session}"),
            credentials,
            None,
        )
        .await
    {
        Ok(dto) => *publication.dto.write().await = dto,
        Err(error) if error.unanswered() => {
            tracing::warn!(%error, "terminal access check got no answer; it will be tried again");
        }
        Err(error) => return Err(error),
    }
    let connected = viewers
        .lock()
        .map(|viewers| viewers.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut identities = BTreeMap::new();
    for viewer in connected {
        if !allowed(publication, credentials, &viewer.user).await {
            viewer.cancel.cancel();
            continue;
        }
        if !identities.contains_key(&viewer.user) {
            let room = publication.info.read().await.mission_id;
            let identity = if let Some(room) = room.filter(|_| viewer.user != credentials.user_id) {
                network.room_identity(credentials, room, &viewer.user).await
            } else {
                network.fetch_identity_with(credentials, &viewer.user).await
            };
            identities.insert(viewer.user.clone(), identity);
        }
        match &identities[&viewer.user] {
            Ok(identity) => {
                if identity
                    .devices
                    .get(&viewer.device)
                    .is_none_or(|certificate| certificate.sig_public_key != viewer.key)
                {
                    viewer.cancel.cancel();
                }
            }
            Err(error) if error.unanswered() => {}
            Err(Error::Stale | Error::SignedOut | Error::Closed) => return Err(Error::Stale),
            Err(_) => viewer.cancel.cancel(),
        }
    }
    close_when_asked(network, publication, credentials).await
}

async fn close_when_asked(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
) -> Result<()> {
    let dto = publication.dto.read().await.clone();
    let Some(request) = &dto.close_request else {
        return Ok(());
    };
    if publication.closing.load(Ordering::Acquire) {
        return Ok(());
    }
    if allowed(publication, credentials, &request.user_id).await
        && !crate::identity::recovery::is_recovery_device(&request.device_id)
    {
        let room = publication.info.read().await.mission_id;
        let key = |identity: &crate::identity::pins::VerifiedIdentity| {
            identity
                .devices
                .get(&request.device_id)
                .map(|certificate| certificate.sig_public_key.clone())
        };
        let mut identity = match room.filter(|_| request.user_id != credentials.user_id) {
            Some(room) => {
                network
                    .room_identity(credentials, room, &request.user_id)
                    .await
            }
            None => network.known_identity(credentials, &request.user_id).await,
        };
        if room.is_none()
            && identity
                .as_ref()
                .is_ok_and(|identity| key(identity).is_none())
        {
            identity = network
                .fetch_identity_with(credentials, &request.user_id)
                .await;
        }
        match identity {
            Ok(identity) => {
                if key(&identity).is_some_and(|public| {
                    crate::network::backend_client::close_signature_holds(&dto, &public)
                }) {
                    return close(publication).await;
                }
            }
            Err(error) if error.unanswered() => return Ok(()),
            Err(Error::Stale | Error::SignedOut | Error::Closed) => return Err(Error::Stale),
            Err(_) => {}
        }
    }
    tracing::warn!(terminal = %dto.id, "a request to close this terminal did not hold; it was removed");
    network.refuse_close(credentials, &dto).await
}

async fn close(publication: &Publication) -> Result<()> {
    let (reply, response) = oneshot::channel();
    tokio::time::timeout(
        Duration::from_secs(5),
        publication.requests.send(HostRequest::Control {
            connection_id: Uuid::nil(),
            control: TerminalControl::Close,
            authorization: CancellationToken::new(),
            reply,
        }),
    )
    .await
    .map_err(|_| Error::Busy)?
    .map_err(|_| Error::Closed)?;
    publication.closing.store(true, Ordering::Release);
    tokio::spawn(tokio::time::timeout(CONTROL_TIMEOUT, response));
    Ok(())
}

async fn viewer(
    network: BackendClient,
    publication: Arc<Publication>,
    credentials: Credentials,
    output: PublicationOutput,
    pipe: Pipe,
    (user, device): (String, String),
    viewers: Viewers,
    cancel: CancellationToken,
) {
    let channel = pipe.id();
    let result = tokio::select! {
        () = cancel.cancelled() => Ok(()),
        result = connect(&network, &publication, &credentials, output, pipe, (user, device), &viewers, &cancel) => result,
    };
    let admitted = viewers
        .lock()
        .ok()
        .and_then(|mut viewers| viewers.remove(&channel));
    if let Some(admitted) = admitted {
        admitted.cancel.cancel();
        let _outcome = tokio::time::timeout(
            Duration::from_millis(500),
            publication.requests.send(HostRequest::Disconnected {
                connection_id: channel,
            }),
        )
        .await;
    }
    if let Err(error) = result
        && !matches!(error, Error::Closed | Error::Stale)
    {
        tracing::warn!(%error, "a terminal viewer connection ended");
    }
}

async fn connect(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
    output: PublicationOutput,
    pipe: Pipe,
    (user, device): (String, String),
    viewers: &Viewers,
    cancel: &CancellationToken,
) -> Result<()> {
    let id = pipe.id();
    let keys = network.channel_keys(credentials)?;
    let mut channel = Channel::accept(pipe, &keys).await?;
    let key = channel.peer_key()?;
    if let Err(refuse) = admit(
        network,
        publication,
        credentials,
        (&user, &device),
        &key,
        viewers,
    )
    .await
    {
        channel.send(&Frame::Refuse(refuse)).await?;
        channel.close().await;
        return Ok(());
    }
    let info = publication.info.read().await.clone();
    let accept = Frame::Accept(Accept {
        protocol_version: crate::protocol::CHANNEL_VERSION,
        session_id: info.session_id,
        incarnation_id: info.incarnation_id,
        host_device_id: credentials.keys.device_id.clone(),
    });
    if channel.version().is_err() {
        // An older viewer reads this version and says that its own Kodosi must update.
        channel.send(&accept).await?;
        channel.close().await;
        return Ok(());
    }
    let admitted = Admitted {
        user: user.clone(),
        device: device.clone(),
        key: key.clone(),
        cancel: cancel.clone(),
    };
    if let Ok(mut viewers) = viewers.lock() {
        viewers.insert(id, admitted);
    }
    channel.send(&accept).await?;
    publication
        .requests
        .send(HostRequest::Connected {
            connection_id: id,
            user_id: user.clone(),
        })
        .await
        .map_err(|_| Error::Closed)?;
    let authorization = cancel.child_token();
    let _authorization = authorization.clone().drop_guard();
    let door = Door::new(
        keys,
        key,
        publication.requests.clone(),
        id,
        authorization.clone(),
    );
    let (ended, channel) = {
        let mut stream = Stream::new(
            channel,
            output,
            &publication.requests,
            Controller {
                connection: id,
                authorization,
            },
            door,
        );
        (stream.serve().await, stream.channel)
    };
    if ended.is_ok() {
        channel.close().await;
    }
    ended
}

fn refuse(code: &str, message: Text) -> Refuse {
    Refuse {
        code: code.to_owned(),
        message,
    }
}

async fn admit(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
    (user, device): (&str, &str),
    key: &[u8],
    viewers: &Viewers,
) -> std::result::Result<(), Refuse> {
    if !allowed(publication, credentials, user).await {
        return Err(refuse(
            "access",
            text!("This terminal is not shared with you."),
        ));
    }
    if crate::identity::recovery::is_recovery_device(device) {
        return Err(refuse("access", text!("This device is not approved.")));
    }
    if viewers.lock().is_ok_and(|viewers| viewers.len() >= VIEWERS) {
        return Err(refuse(
            "busy",
            text!("Too many devices are connected to this terminal."),
        ));
    }
    let approved = |identity: &crate::identity::pins::VerifiedIdentity| {
        identity
            .devices
            .get(device)
            .is_some_and(|certificate| certificate.sig_public_key == key)
    };
    let room = publication.info.read().await.mission_id;
    let mut identity = if let Some(room) = room.filter(|_| user != credentials.user_id) {
        network.room_identity(credentials, room, user).await
    } else {
        network.known_identity(credentials, user).await
    };
    if room.is_none() && identity.as_ref().is_ok_and(|identity| !approved(identity)) {
        identity = network.fetch_identity_with(credentials, user).await;
    }
    match identity {
        Ok(identity) if approved(&identity) => Ok(()),
        Err(error) if error.unanswered() => Err(refuse(
            "busy",
            text!("The host could not check this device. Try again."),
        )),
        Err(Error::Trust(_)) => Err(refuse(
            "access",
            text!("The owner of this terminal must trust your identity in Friends first."),
        )),
        Ok(_) | Err(_) => Err(refuse("access", text!("This device is not approved."))),
    }
}

const LEAVING_FRAMES: usize = 64;
const PIPE_PARTS_WAITING: usize = 256;

struct Controller {
    connection: Uuid,
    authorization: CancellationToken,
}

enum Waiting {
    Output {
        sequence: u64,
        bytes: Bytes,
    },
    Resize {
        rows: u16,
        cols: u16,
        cell: crate::terminal::CellPixels,
        at_sequence: u64,
    },
}

#[derive(PartialEq, Eq)]
enum Exact {
    Held,
    Owed,
    Due,
}

enum Captured {
    Cut(CheckpointCut),
    Again,
    Ended(End),
}

/// What the documents of this connection give it to send.
enum Leaving {
    Frame { pipe: Uuid, frame: Vec<u8> },
    Closed { pipe: Uuid },
}

struct PipeIn {
    sender: mpsc::Sender<Vec<u8>>,
    filling: Vec<u8>,
}

/// Carries a document's frames to the connection until the document or the connection ends.
async fn leave(pipe: Uuid, mut frames: mpsc::Receiver<Vec<u8>>, leaving: mpsc::Sender<Leaving>) {
    while let Some(frame) = frames.recv().await {
        if leaving.send(Leaving::Frame { pipe, frame }).await.is_err() {
            return;
        }
    }
    drop(leaving.send(Leaving::Closed { pipe }).await);
}

struct Stream<'a, T> {
    channel: Channel<T>,
    output: PublicationOutput,
    requests: &'a mpsc::Sender<HostRequest>,
    controller: Controller,
    door: Door,
    pacer: Pacer,
    parts: VecDeque<Bytes>,
    waiting: VecDeque<Waiting>,
    waiting_bytes: usize,
    live: bool,
    exact: Exact,
    next_sequence: u64,
    output_at: tokio::time::Instant,
    heard: tokio::time::Instant,
    keyframe_bytes: usize,
    keyframe_at: Option<tokio::time::Instant>,
    metadata: Option<TerminalMetadata>,
    heartbeat: u32,
    input_offset: u64,
    input_confirmed: u64,
    inputs: Inputs,
    controls: Controls,
    pipes: HashMap<Uuid, PipeIn>,
    pipe_parts: VecDeque<Frame>,
    leaving: mpsc::Receiver<Leaving>,
    leaving_to: mpsc::Sender<Leaving>,
    picture: Option<Receiving>,
    picture_answer: Option<PictureAnswer>,
}

enum Skip {
    Reached,
    Lost,
    Ended(End),
}

impl<'a, T: Transport> Stream<'a, T> {
    fn new(
        channel: Channel<T>,
        output: PublicationOutput,
        requests: &'a mpsc::Sender<HostRequest>,
        controller: Controller,
        door: Door,
    ) -> Self {
        let (leaving_to, leaving) = mpsc::channel(LEAVING_FRAMES);
        Self {
            channel,
            output,
            requests,
            controller,
            door,
            pacer: Pacer::default(),
            parts: VecDeque::new(),
            waiting: VecDeque::new(),
            waiting_bytes: 0,
            live: false,
            exact: Exact::Owed,
            next_sequence: 0,
            output_at: tokio::time::Instant::now(),
            heard: tokio::time::Instant::now(),
            keyframe_bytes: 0,
            keyframe_at: None,
            metadata: None,
            heartbeat: 0,
            input_offset: 0,
            input_confirmed: 0,
            inputs: Inputs::new(),
            controls: Controls::new(),
            pipes: HashMap::new(),
            pipe_parts: VecDeque::new(),
            leaving,
            leaving_to,
            picture: None,
            picture_answer: None,
        }
    }

    async fn serve(&mut self) -> Result<()> {
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut metadata = tokio::time::interval(Duration::from_secs(1));
        metadata.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if let Some(end) = self.advance().await? {
                self.channel.send(&Frame::End(end)).await?;
                return Ok(());
            }
            let idle = self.parts.is_empty() && self.pacer.in_transit() == 0;
            let capture = (!self.live && idle)
                .then(|| self.keyframe_at.map(|at| at + KEYFRAME_SPACING))
                .flatten();
            let exact = (self.exact == Exact::Owed && self.live && idle && self.waiting.is_empty())
                .then(|| {
                    let quiet = self.output_at + EXACT_AFTER_QUIET;
                    self.keyframe_at
                        .map_or(quiet, |at| quiet.max(at + KEYFRAME_SPACING))
                });
            tokio::select! {
                biased;
                () = self.controller.authorization.cancelled() => return Ok(()),
                Some((offset, accepted)) = self.inputs.next(), if !self.inputs.is_empty() => {
                    if !accepted {
                        return Err(Error::Text(text!("The terminal did not accept input from this connection.")));
                    }
                    self.input_confirmed = offset;
                    self.channel.send(&Frame::InputAck { offset }).await?;
                }
                Some(answer) = self.controls.next(), if !self.controls.is_empty() => {
                    self.channel.send(&answer).await?;
                }
                answer = async { match self.picture_answer.as_mut() { Some(answer) => answer.await, None => std::future::pending().await } }, if self.picture_answer.is_some() => {
                    self.picture_answer = None;
                    self.picture_answered(answer).await?;
                }
                _ = heartbeat.tick() => {
                    if self.heard.elapsed() >= CHANNEL_SILENCE {
                        return Err(invalid("The viewer did not answer."));
                    }
                    self.heartbeat = self.heartbeat.wrapping_add(1);
                    self.channel.send(&Frame::Heartbeat { number: self.heartbeat, next_sequence: self.next_sequence }).await?;
                }
                _ = metadata.tick(), if self.metadata.is_some() && self.live => {
                    if let Some(current) = self.metadata.take() {
                        self.channel.send(&Frame::Metadata(current)).await?;
                    }
                }
                frame = self.channel.receive() => match frame? {
                    Some((frame, _)) => {
                        self.heard = tokio::time::Instant::now();
                        self.received(frame).await?;
                    }
                    None => return Err(Error::Closed),
                },
                published = self.output.recv() => {
                    let mut next = Some(published);
                    for _ in 0..wire::RAW_BATCH_LIMIT {
                        let Some(published) = next.take() else { break };
                        if let Some(end) = self.published(published) {
                            self.drain().await?;
                            self.channel.send(&Frame::End(end)).await?;
                            return Ok(());
                        }
                        next = match self.output.try_recv() {
                            Ok(frame) => Some(Ok(frame)),
                            Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                                Some(Err(broadcast::error::RecvError::Lagged(missed)))
                            }
                            Err(broadcast::error::TryRecvError::Closed) => {
                                Some(Err(broadcast::error::RecvError::Closed))
                            }
                            Err(broadcast::error::TryRecvError::Empty) => None,
                        };
                    }
                }
                Some(left) = self.leaving.recv(), if self.pipe_parts.len() < PIPE_PARTS_WAITING => {
                    self.left(left);
                }
                () = async { tokio::time::sleep_until(capture.unwrap_or_else(tokio::time::Instant::now)).await }, if capture.is_some() => {}
                () = async { tokio::time::sleep_until(exact.unwrap_or_else(tokio::time::Instant::now)).await }, if exact.is_some() => {
                    self.live = false;
                    self.exact = Exact::Due;
                }
            }
        }
    }

    async fn advance(&mut self) -> Result<Option<End>> {
        let end = self.advance_terminal().await?;
        if end.is_none() {
            self.send_pipe_parts().await?;
        }
        Ok(end)
    }

    /// Terminal output goes first. Documents use the room that it leaves.
    async fn send_pipe_parts(&mut self) -> Result<()> {
        while !self.pipe_parts.is_empty()
            && self.parts.is_empty()
            && (!self.live || self.waiting.is_empty())
            && self.pacer.room()
        {
            let Some(frame) = self.pipe_parts.pop_front() else {
                break;
            };
            let sent = self.channel.send(&frame).await?;
            if frame.carries_output() {
                self.pacer.sent(sent as u64, tokio::time::Instant::now());
            }
        }
        Ok(())
    }

    fn left(&mut self, left: Leaving) {
        match left {
            Leaving::Closed { pipe } => {
                self.pipes.remove(&pipe);
                self.pipe_parts.push_back(Frame::PipeClose { pipe });
            }
            Leaving::Frame { pipe, frame } => {
                let parts = frame.len().div_ceil(wire::PIPE_PART);
                for (index, part) in frame.chunks(wire::PIPE_PART).enumerate() {
                    self.pipe_parts.push_back(Frame::PipeData {
                        pipe,
                        more: index + 1 < parts,
                        part: Bytes::copy_from_slice(part),
                    });
                }
            }
        }
    }

    /// Asks the terminal for a document. A slow open does not hold the terminal, and frames
    /// that the viewer sends in that time wait in the pipe.
    fn open_pipe(&mut self, pipe: Uuid, key: String, name: String) {
        if self.pipes.len() >= MOST_PIPES {
            self.pipe_parts.push_back(Frame::PipeClose { pipe });
            return;
        }
        let (sender, mut waiting) = mpsc::channel(PIPE_QUEUE);
        self.pipes.insert(
            pipe,
            PipeIn {
                sender,
                filling: Vec::new(),
            },
        );
        let requests = self.requests.clone();
        let leaving = self.leaving_to.clone();
        let connection_id = self.controller.connection;
        tokio::spawn(async move {
            let (reply, response) = oneshot::channel();
            let request = HostRequest::Document {
                connection_id,
                key,
                name,
                reply,
            };
            let ends = match requests.send(request).await {
                Ok(()) => response.await.ok().and_then(std::result::Result::ok),
                Err(_) => None,
            };
            let Some(ends) = ends else {
                drop(leaving.send(Leaving::Closed { pipe }).await);
                return;
            };
            tokio::spawn(leave(pipe, ends.receiver, leaving));
            while let Some(frame) = waiting.recv().await {
                if ends.sender.send(frame).await.is_err() {
                    return;
                }
            }
        });
    }

    fn pipe_part(&mut self, pipe: Uuid, more: bool, part: &Bytes) -> Result<()> {
        let Some(open) = self.pipes.get_mut(&pipe) else {
            return Ok(());
        };
        if open.filling.len() + part.len() > MOST_PIPE_FRAME {
            return Err(invalid("A document frame exceeds its bound."));
        }
        open.filling.extend_from_slice(part);
        if !more
            && open
                .sender
                .try_send(std::mem::take(&mut open.filling))
                .is_err()
        {
            self.pipes.remove(&pipe);
            self.pipe_parts.push_back(Frame::PipeClose { pipe });
        }
        Ok(())
    }

    async fn advance_terminal(&mut self) -> Result<Option<End>> {
        loop {
            if !self.live && self.parts.is_empty() && self.pacer.in_transit() == 0 {
                self.pacer.idle();
                if self
                    .keyframe_at
                    .is_some_and(|at| at.elapsed() < KEYFRAME_SPACING)
                {
                    return Ok(None);
                }
                if let Some(end) = self.capture().await? {
                    return Ok(Some(end));
                }
                if !self.live {
                    return Ok(None);
                }
                continue;
            }
            let ready = !self.parts.is_empty() || (self.live && !self.waiting.is_empty());
            if !ready {
                if self.live {
                    self.pacer.idle();
                } else {
                    self.pacer.blocked();
                }
                return Ok(None);
            }
            if !self.pacer.room() {
                return Ok(None);
            }
            self.send_next().await?;
        }
    }

    async fn send_next(&mut self) -> Result<()> {
        let frame = if let Some(part) = self.parts.pop_front() {
            Frame::Keyframe {
                next_sequence: self.next_sequence,
                more: !self.parts.is_empty(),
                part,
            }
        } else {
            match self.waiting.pop_front() {
                Some(Waiting::Resize {
                    rows,
                    cols,
                    cell,
                    at_sequence,
                }) => Frame::Resize {
                    rows,
                    cols,
                    cell,
                    at_sequence,
                },
                Some(Waiting::Output { sequence, bytes }) => {
                    let limit = wire::OUTPUT_FRAME.min(self.window());
                    let mut size = bytes.len();
                    self.waiting_bytes -= bytes.len();
                    let mut chunks = vec![bytes];
                    while chunks.len() < wire::RAW_BATCH_LIMIT
                        && let Some(Waiting::Output { bytes, .. }) = self.waiting.front()
                        && size + bytes.len() <= limit
                        && let Some(Waiting::Output { bytes, .. }) = self.waiting.pop_front()
                    {
                        size += bytes.len();
                        self.waiting_bytes -= bytes.len();
                        chunks.push(bytes);
                    }
                    self.next_sequence = sequence + chunks.len() as u64;
                    Frame::Output {
                        first_sequence: sequence,
                        chunks,
                    }
                }
                None => return Ok(()),
            }
        };
        let sent = self.channel.send(&frame).await?;
        if frame.carries_output() {
            self.pacer.sent(sent as u64, tokio::time::Instant::now());
        }
        if matches!(frame, Frame::Keyframe { .. }) {
            self.pacer.bulk();
        }
        Ok(())
    }

    async fn cut(&mut self, full: bool) -> Captured {
        self.keyframe_at = Some(tokio::time::Instant::now());
        let marker = Uuid::now_v7();
        let (reply, response) = oneshot::channel();
        let request = HostRequest::Bootstrap {
            request_id: marker,
            history: full,
            image_data_bytes: self.pacer.image_data_bytes(),
            reply,
        };
        if self.requests.try_send(request).is_err() {
            return Captured::Again;
        }
        let cut = match tokio::time::timeout(Duration::from_secs(5), response).await {
            Ok(Ok(Ok(cut))) => cut,
            Ok(Ok(Err(reason))) => {
                tracing::warn!(%reason, "a terminal snapshot for a viewer failed; it will be tried again");
                return Captured::Again;
            }
            _ => return Captured::Again,
        };
        match self.skip(marker) {
            Skip::Reached => Captured::Cut(cut),
            Skip::Lost => Captured::Again,
            Skip::Ended(end) => Captured::Ended(end),
        }
    }

    async fn capture(&mut self) -> Result<Option<End>> {
        let full = self.exact == Exact::Due
            || (self.keyframe_bytes > 0 && self.keyframe_bytes <= self.window() / 2);
        let cut = match self.cut(full).await {
            Captured::Cut(cut) => cut,
            Captured::Again => return Ok(None),
            Captured::Ended(end) => return Ok(Some(end)),
        };
        self.parts = wire::snapshot_parts(&cut.checkpoint)?.into();
        if full {
            self.keyframe_bytes = self.parts.iter().map(Bytes::len).sum();
        }
        self.next_sequence = cut.next_sequence;
        self.waiting.clear();
        self.waiting_bytes = 0;
        self.metadata = None;
        self.live = true;
        self.exact = if full { Exact::Held } else { Exact::Owed };
        Ok(None)
    }

    fn skip(&mut self, marker: Uuid) -> Skip {
        loop {
            match self.output.try_recv() {
                Ok(PublishedFrame::BootstrapBarrier { request_id }) if request_id == marker => {
                    return Skip::Reached;
                }
                Ok(PublishedFrame::Closed {
                    reason,
                    final_sequence,
                }) => {
                    return Skip::Ended(End {
                        final_sequence,
                        reason,
                    });
                }
                Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => return Skip::Lost,
            }
        }
    }

    fn window(&self) -> usize {
        usize::try_from(self.pacer.window()).unwrap_or(usize::MAX)
    }

    fn behind(&mut self) {
        self.live = false;
        self.waiting.clear();
        self.waiting_bytes = 0;
    }

    fn published(
        &mut self,
        published: std::result::Result<PublishedFrame, broadcast::error::RecvError>,
    ) -> Option<End> {
        match published {
            Ok(PublishedFrame::Metadata(current)) => self.metadata = Some(current),
            Ok(PublishedFrame::BootstrapBarrier { .. }) => {}
            Ok(PublishedFrame::Raw { sequence, bytes }) => {
                if !self.live {
                    return None;
                }
                let expected = self.next_waiting();
                if sequence < expected {
                    return None;
                }
                if sequence != expected {
                    self.behind();
                    return None;
                }
                self.output_at = tokio::time::Instant::now();
                self.waiting_bytes += bytes.len();
                self.waiting.push_back(Waiting::Output { sequence, bytes });
                if self.waiting_bytes > self.window() {
                    self.behind();
                }
            }
            Ok(PublishedFrame::Resize {
                rows,
                cols,
                cell,
                at_sequence,
            }) => {
                if self.live && at_sequence >= self.next_waiting() {
                    self.waiting.push_back(Waiting::Resize {
                        rows,
                        cols,
                        cell,
                        at_sequence,
                    });
                }
            }
            Ok(PublishedFrame::Closed {
                reason,
                final_sequence,
            }) => {
                return Some(End {
                    final_sequence,
                    reason,
                });
            }
            Err(broadcast::error::RecvError::Lagged(_)) => self.behind(),
            Err(broadcast::error::RecvError::Closed) => {
                return Some(End {
                    final_sequence: self.next_sequence,
                    reason: text!("The terminal closed."),
                });
            }
        }
        None
    }

    fn next_waiting(&self) -> u64 {
        self.waiting
            .iter()
            .rev()
            .find_map(|waiting| match waiting {
                Waiting::Output { sequence, .. } => Some(sequence + 1),
                Waiting::Resize { .. } => None,
            })
            .unwrap_or(self.next_sequence)
    }

    async fn drain(&mut self) -> Result<()> {
        while !self.parts.is_empty() || (self.live && !self.waiting.is_empty()) {
            self.send_next().await?;
        }
        Ok(())
    }

    async fn received(&mut self, frame: Frame) -> Result<()> {
        match frame {
            Frame::Ack { received } => self
                .pacer
                .acknowledge(received, tokio::time::Instant::now())?,
            Frame::Refresh => {
                if self.exact == Exact::Held {
                    self.exact = Exact::Owed;
                }
                self.behind();
            }
            Frame::Input {
                offset,
                heartbeat,
                bytes,
            } => {
                if offset != self.input_offset
                    || self.input_offset - self.input_confirmed > UNCONFIRMED_INPUT
                {
                    return Err(invalid("Terminal input is out of order."));
                }
                if self.heartbeat.wrapping_sub(heartbeat) > LATE_HEARTBEATS {
                    return Err(Error::Text(text!(
                        "Terminal input arrived too late, and the host did not use it."
                    )));
                }
                self.input_offset += bytes.len() as u64;
                let end = self.input_offset;
                let response = self
                    .submit(TerminalControl::Input {
                        bytes: bytes.to_vec(),
                    })
                    .await?;
                self.inputs.push_back(Box::pin(async move {
                    (end, matches!(response.await, Ok(Ok(_))))
                }));
            }
            Frame::Control {
                request_id,
                control,
            } => {
                if self.controls.len() >= 64 {
                    return Err(Error::Busy);
                }
                let answers = matches!(control, TerminalControl::Canvas { .. });
                let response = self.submit(control).await?;
                let authorization = self.controller.authorization.clone();
                self.controls.push(Box::pin(async move {
                    let result = tokio::select! {
                        () = authorization.cancelled() => Err(text!("This terminal connection is no longer authorized.")),
                        result = tokio::time::timeout(CONTROL_TIMEOUT, response) => match result {
                            Ok(Ok(result)) => result,
                            _ => Err(text!("The terminal did not confirm the operation, and Kodosi did not send it again.")),
                        },
                    };
                    match result {
                        Ok(value) if answers && wire::answer_fits(&value) => {
                            Frame::ControlValue { request_id, value }
                        }
                        Ok(_) if answers => Frame::ControlResult(ControlResult {
                            request_id,
                            accepted: false,
                            message: Some(text!("The answer does not fit one message.")),
                        }),
                        result => Frame::ControlResult(ControlResult {
                            request_id,
                            accepted: result.is_ok(),
                            message: result.err(),
                        }),
                    }
                }));
            }
            Frame::PipeOpen { pipe, key, name } => self.open_pipe(pipe, key, name),
            Frame::PipeData { pipe, more, part } => self.pipe_part(pipe, more, &part)?,
            Frame::PipeClose { pipe } => {
                self.pipes.remove(&pipe);
            }
            Frame::DirectAsk { pipe } => {
                let addresses = self.door.open().await;
                self.channel
                    .send(&Frame::DirectOffer { pipe, addresses })
                    .await?;
            }
            Frame::Picture(offer) => self.offered(offer).await?,
            Frame::PicturePart { id, offset, bytes } => self.part(id, offset, &bytes).await?,
            _ => return Err(invalid("Unsupported terminal viewer frame.")),
        }
        Ok(())
    }

    /// A view offers a picture. The terminal pastes it at once if it has its file. A view offers
    /// the next picture before an answer only when it stopped waiting for the last one, so the
    /// next picture takes its place.
    async fn offered(&mut self, offer: PictureOffer) -> Result<()> {
        let digest = crate::terminal::picture_digest(&offer.sha256)
            .filter(|_| offer.size > 0 && offer.size <= TERMINAL_PICTURE_MAX_BYTES as u64)
            .ok_or_else(|| invalid("The picture offer is not valid."))?;
        self.picture = None;
        self.picture_answer = None;
        let reply = self
            .submit_picture(HostPicture::Known {
                digest,
                format: offer.format,
            })
            .await?;
        self.picture_answer = Some(Box::pin(async move {
            let known = match tokio::time::timeout(CONTROL_TIMEOUT, reply).await {
                Ok(Ok(Ok(answer))) => Ok(answer.get("missing") != Some(&Value::Bool(true))),
                Ok(Ok(Err(message))) => Err(message),
                _ => Err(text!("The terminal did not take the picture.")),
            };
            (offer, known)
        }));
        Ok(())
    }

    async fn part(&mut self, id: Uuid, offset: u64, bytes: &[u8]) -> Result<()> {
        let Some(receiving) = self
            .picture
            .as_mut()
            .filter(|receiving| receiving.offer.id == id && offset == receiving.bytes.len() as u64)
        else {
            return Err(invalid("A picture part arrived out of order."));
        };
        if offset + bytes.len() as u64 > receiving.offer.size {
            return Err(invalid("A picture part exceeds its picture."));
        }
        receiving.bytes.extend_from_slice(bytes);
        let received = receiving.bytes.len() as u64;
        self.channel
            .send(&Frame::PictureState(PictureState {
                id,
                received,
                at: u64::try_from(receiving.asked.elapsed().as_micros()).unwrap_or(u64::MAX),
                step: PictureStep::Receiving,
                message: None,
            }))
            .await?;
        if received == receiving.offer.size
            && let Some(receiving) = self.picture.take()
        {
            self.paste(receiving).await?;
        }
        Ok(())
    }

    /// Pastes a picture whose parts all arrived, if it is the picture that the view offered.
    async fn paste(&mut self, receiving: Receiving) -> Result<()> {
        let Receiving {
            offer,
            bytes,
            _memory: memory,
            ..
        } = receiving;
        let picture = Picture::new(Bytes::from(bytes))
            .ok()
            .filter(|picture| picture.sha256() == offer.sha256 && picture.format == offer.format);
        let Some(picture) = picture else {
            return self
                .picture_answered((
                    offer,
                    Err(text!("The picture arrived damaged. Paste it again.")),
                ))
                .await;
        };
        let reply = self.submit_picture(HostPicture::Whole(picture)).await?;
        self.picture_answer = Some(Box::pin(async move {
            let pasted = match tokio::time::timeout(CONTROL_TIMEOUT, reply).await {
                Ok(Ok(Ok(_))) => Ok(true),
                Ok(Ok(Err(message))) => Err(message),
                _ => Err(text!("The terminal did not take the picture.")),
            };
            drop(memory);
            (offer, pasted)
        }));
        Ok(())
    }

    /// Answers a view's picture: pasted, refused, or asked for when this host does not have it.
    async fn picture_answered(
        &mut self,
        (offer, answer): (PictureOffer, std::result::Result<bool, Text>),
    ) -> Result<()> {
        let (id, size) = (offer.id, offer.size);
        let state = move |step, message: Option<Text>| {
            Frame::PictureState(PictureState {
                id,
                received: if step == PictureStep::Pasted { size } else { 0 },
                at: 0,
                step,
                message,
            })
        };
        let frame = match answer {
            Ok(true) => state(PictureStep::Pasted, None),
            Err(message) => state(PictureStep::Refused, Some(message)),
            Ok(false) => {
                let memory = u32::try_from(offer.size)
                    .ok()
                    .and_then(|size| PICTURE_MEMORY.try_acquire_many(size).ok());
                match memory {
                    Some(memory) => {
                        let frame = state(PictureStep::Receiving, None);
                        self.picture = Some(Receiving {
                            bytes: Vec::with_capacity(
                                usize::try_from(offer.size).unwrap_or_default(),
                            ),
                            offer,
                            asked: tokio::time::Instant::now(),
                            _memory: memory,
                        });
                        frame
                    }
                    None => state(
                        PictureStep::Refused,
                        Some(text!(
                            "The other computer is receiving other pictures. Paste it again in a moment."
                        )),
                    ),
                }
            }
        };
        self.channel.send(&frame).await.map(|_| ())
    }

    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "exclusive borrow keeps the non-Sync pending futures Send across the await"
    )]
    async fn submit_picture(&mut self, picture: HostPicture) -> Result<Reply> {
        let (reply, response) = oneshot::channel();
        tokio::time::timeout(
            Duration::from_secs(5),
            self.requests.send(HostRequest::Picture {
                connection_id: self.controller.connection,
                authorization: self.controller.authorization.clone(),
                picture,
                reply,
            }),
        )
        .await
        .map_err(|_| Error::Busy)?
        .map_err(|_| Error::Closed)?;
        Ok(response)
    }

    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "exclusive borrow keeps the non-Sync pending futures Send across the await"
    )]
    async fn submit(&mut self, control: TerminalControl) -> Result<Reply> {
        let (reply, response) = oneshot::channel();
        tokio::time::timeout(
            Duration::from_secs(5),
            self.requests.send(HostRequest::Control {
                connection_id: self.controller.connection,
                control,
                authorization: self.controller.authorization.clone(),
                reply,
            }),
        )
        .await
        .map_err(|_| Error::Busy)?
        .map_err(|_| Error::Closed)?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests;
