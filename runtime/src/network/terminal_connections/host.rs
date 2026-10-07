use std::collections::VecDeque;

use futures_util::stream::{FuturesOrdered, FuturesUnordered};
use tokio::task::JoinSet;

use super::{
    channel::{Channel, Transport},
    link::{HostEvent, Pipe},
    pacer::Pacer,
    wire::{Accept, ControlResult, End, Frame, Refuse},
    *,
};
use crate::{network::CheckpointCut, terminal::TerminalMetadata};

const VIEWERS: usize = 32;
const HEARTBEAT: Duration = Duration::from_secs(15);
const LATE_HEARTBEATS: u32 = 2;
const KEYFRAME_SPACING: Duration = Duration::from_millis(500);
const EXACT_AFTER_QUIET: Duration = Duration::from_secs(1);
const UNCONFIRMED_INPUT: u64 = 256 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(12);

type Reply = oneshot::Receiver<std::result::Result<Value, String>>;
type Inputs = FuturesOrdered<std::pin::Pin<Box<dyn Future<Output = (u64, bool)> + Send>>>;
type Controls = FuturesUnordered<std::pin::Pin<Box<dyn Future<Output = ControlResult> + Send>>>;

#[derive(Clone)]
struct Admitted {
    user: String,
    device: String,
    key: Vec<u8>,
    cancel: CancellationToken,
}

type Viewers = Arc<std::sync::Mutex<BTreeMap<Uuid, Admitted>>>;

pub(super) async fn run(
    network: &BackendClient,
    publication: &Arc<Publication>,
    output: &mut PublicationOutput,
    generation: u64,
) -> Result<()> {
    check_generation(network, generation)?;
    let credentials = network.credentials()?;
    let current = current_publication(network, publication, &credentials).await?;
    let mut hosted = network.inner.link.host(current.id, current.incarnation_id);
    check_generation(network, generation)?;
    drop(publication.requests.send(HostRequest::ResetPresence).await);
    let viewers = Viewers::default();
    let stop = credentials.cancel.child_token();
    let _stop = stop.clone().drop_guard();
    let mut tasks = JoinSet::new();
    let mut trust = tokio::time::interval(TRUST_CHECK);
    trust.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    trust.tick().await;
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
    drop(publication.requests.send(HostRequest::ResetPresence).await);
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
            if dto.mission_id.is_some() {
                let mut local = publication.info.write().await;
                local.mission_id = dto.mission_id;
                local.shared_with = dto.shared_with.iter().cloned().collect();
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
            let dto:SessionDto=network.inner.http.device(Method::POST,"api/sessions",credentials,Some(json!({"id":info.session_id,"incarnationId":info.incarnation_id,"name":info.name,"hostDeviceId":credentials.keys.device_id,"hostName":crate::network::backend_client::host_label(),"missionId":null}))).await?;
            *publication.dto.write().await = dto.clone();
            network.emit_for(credentials.generation,Some(credentials.user_id.clone()),json!({"type":"system.error","message":"The server no longer had this terminal; choose friends again."}));
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
    let mut channel = Channel::accept(pipe, &*network.channel_keys(credentials)?).await?;
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
    let admitted = Admitted {
        user: user.clone(),
        device: device.clone(),
        key,
        cancel: cancel.clone(),
    };
    if let Ok(mut viewers) = viewers.lock() {
        viewers.insert(id, admitted);
    }
    let info = publication.info.read().await.clone();
    channel
        .send(&Frame::Accept(Accept {
            protocol_version: crate::protocol::TERMINAL_CONNECTION_VERSION,
            session_id: info.session_id,
            incarnation_id: info.incarnation_id,
            host_device_id: credentials.keys.device_id.clone(),
        }))
        .await?;
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
    let mut stream = Stream::new(
        channel,
        output,
        &publication.requests,
        Controller {
            user,
            device,
            connection: id,
            authorization,
        },
    );
    let ended = stream.serve().await;
    if ended.is_ok() {
        stream.channel.close().await;
    }
    ended
}

fn refuse(code: &str, message: &str) -> Refuse {
    Refuse {
        code: code.to_owned(),
        message: message.to_owned(),
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
        return Err(refuse("access", "This terminal is not shared with you."));
    }
    if viewers.lock().is_ok_and(|viewers| viewers.len() >= VIEWERS) {
        return Err(refuse(
            "busy",
            "Too many devices are connected to this terminal.",
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
            "The host could not check this device; try again.",
        )),
        Err(Error::Trust(_)) => Err(refuse(
            "access",
            "The owner of this terminal must trust your identity in Friends first.",
        )),
        Ok(_) | Err(_) => Err(refuse("access", "This device is not approved.")),
    }
}

struct Controller {
    user: String,
    device: String,
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

struct Stream<'a, T> {
    channel: Channel<T>,
    output: PublicationOutput,
    requests: &'a mpsc::Sender<HostRequest>,
    controller: Controller,
    pacer: Pacer,
    parts: VecDeque<Bytes>,
    waiting: VecDeque<Waiting>,
    waiting_bytes: usize,
    live: bool,
    exact: Exact,
    next_sequence: u64,
    output_at: tokio::time::Instant,
    keyframe_bytes: usize,
    keyframe_at: Option<tokio::time::Instant>,
    metadata: Option<TerminalMetadata>,
    heartbeat: u32,
    input_offset: u64,
    input_confirmed: u64,
    inputs: Inputs,
    controls: Controls,
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
    ) -> Self {
        Self {
            channel,
            output,
            requests,
            controller,
            pacer: Pacer::default(),
            parts: VecDeque::new(),
            waiting: VecDeque::new(),
            waiting_bytes: 0,
            live: false,
            exact: Exact::Owed,
            next_sequence: 0,
            output_at: tokio::time::Instant::now(),
            keyframe_bytes: 0,
            keyframe_at: None,
            metadata: None,
            heartbeat: 0,
            input_offset: 0,
            input_confirmed: 0,
            inputs: Inputs::new(),
            controls: Controls::new(),
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
                        return Err(invalid("The terminal did not accept input from this connection."));
                    }
                    self.input_confirmed = offset;
                    self.channel.send(&Frame::InputAck { offset }).await?;
                }
                Some(result) = self.controls.next(), if !self.controls.is_empty() => {
                    self.channel.send(&Frame::ControlResult(result)).await?;
                }
                _ = heartbeat.tick() => {
                    self.heartbeat = self.heartbeat.wrapping_add(1);
                    self.channel.send(&Frame::Heartbeat { number: self.heartbeat, next_sequence: self.next_sequence }).await?;
                }
                _ = metadata.tick(), if self.metadata.is_some() && self.live => {
                    if let Some(current) = self.metadata.take() {
                        self.channel.send(&Frame::Metadata(current)).await?;
                    }
                }
                frame = self.channel.receive() => match frame? {
                    Some((frame, _)) => self.received(frame).await?,
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
                () = async { tokio::time::sleep_until(capture.unwrap_or_else(tokio::time::Instant::now)).await }, if capture.is_some() => {}
                () = async { tokio::time::sleep_until(exact.unwrap_or_else(tokio::time::Instant::now)).await }, if exact.is_some() => {
                    self.live = false;
                    self.exact = Exact::Due;
                }
            }
        }
    }

    async fn advance(&mut self) -> Result<Option<End>> {
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
                    at_sequence,
                }) => Frame::Resize {
                    rows,
                    cols,
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
                at_sequence,
            }) => {
                if self.live && at_sequence >= self.next_waiting() {
                    self.waiting.push_back(Waiting::Resize {
                        rows,
                        cols,
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
                    reason: "Terminal closed.".to_owned(),
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
                    return Err(invalid("Terminal input arrived too late; it was not used."));
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
                let response = self.submit(control).await?;
                let authorization = self.controller.authorization.clone();
                self.controls.push(Box::pin(async move {
                    let result = tokio::select! {
                        () = authorization.cancelled() => Err("This terminal connection is no longer authorized.".to_owned()),
                        result = tokio::time::timeout(CONTROL_TIMEOUT, response) => match result {
                            Ok(Ok(result)) => result.map(|_| ()),
                            _ => Err("The terminal did not confirm the operation; it was not retried.".to_owned()),
                        },
                    };
                    ControlResult {
                        request_id,
                        accepted: result.is_ok(),
                        message: result.err().unwrap_or_default(),
                    }
                }));
            }
            _ => return Err(invalid("Unsupported terminal viewer frame.")),
        }
        Ok(())
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
                sender_user_id: self.controller.user.clone(),
                sender_device_id: self.controller.device.clone(),
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
