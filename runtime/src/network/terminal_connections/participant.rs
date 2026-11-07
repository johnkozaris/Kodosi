use std::collections::VecDeque;

use super::{
    channel::{Channel, Transport},
    direct,
    link::Pipe,
    wire::{ControlResult, Frame, PICTURE_PART, PictureOffer, PictureState, PictureStep},
    *,
};
use crate::network::untrusted;
use crate::text::Text;

const ACKNOWLEDGE_BYTES: u64 = 4 * 1024;
const ACKNOWLEDGE_DELAY: Duration = Duration::from_millis(100);
const UNCONFIRMED_INPUT: u64 = 64 * 1024;
const PROGRESS_SPACING: Duration = Duration::from_millis(100);
const PICTURE_TRANSIT_SECONDS: f64 = 0.5;

fn picture_not_confirmed() -> Text {
    text!("The other computer did not confirm the picture. Paste it again.")
}

/// A picture on its way to the host. The host asks for its parts when it does not have the
/// picture. In transit are twice the bytes that the link carries in the shortest round trip of
/// a part, and at most half a second of them, as of output: so the link stays full, and input
/// waits behind the parts for about one round trip. The speed is how fast the parts arrived on
/// the host's clock, which answers that come late or together do not change.
struct Upload {
    id: Uuid,
    picture: crate::terminal::Picture,
    reply: oneshot::Sender<Result<()>>,
    asked: bool,
    sent: u64,
    received: u64,
    /// Each part in transit: where it ends and when it left.
    parts: VecDeque<(u64, tokio::time::Instant)>,
    /// The host's recent answers: when it had the bytes, in microseconds, and how many.
    arrivals: VecDeque<(u64, u64)>,
    round_trip: Duration,
    speed: f64,
    told: tokio::time::Instant,
    deadline: tokio::time::Instant,
}

impl Upload {
    fn new(
        id: Uuid,
        picture: crate::terminal::Picture,
        reply: oneshot::Sender<Result<()>>,
        now: tokio::time::Instant,
    ) -> Self {
        Self {
            id,
            picture,
            reply,
            asked: false,
            sent: 0,
            received: 0,
            parts: VecDeque::new(),
            arrivals: VecDeque::new(),
            round_trip: Duration::MAX,
            speed: 0.0,
            told: now,
            deadline: now + CONTROL_TIMEOUT,
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a window does not need exact byte counts"
    )]
    fn wants_part(&self) -> bool {
        let transit = (2.0 * self.round_trip.as_secs_f64()).min(PICTURE_TRANSIT_SECONDS);
        self.asked
            && self.sent < self.picture.bytes.len() as u64
            && (self.sent == self.received
                || ((self.sent - self.received) as f64) < self.speed * transit)
    }

    fn part(&mut self, now: tokio::time::Instant) -> Result<Frame> {
        let start = usize::try_from(self.sent).map_err(|_| Error::Text(picture_not_confirmed()))?;
        let end = self.picture.bytes.len().min(start + PICTURE_PART);
        let frame = Frame::PicturePart {
            id: self.id,
            offset: self.sent,
            bytes: self.picture.bytes.slice(start..end),
        };
        self.sent = end as u64;
        self.parts.push_back((self.sent, now));
        Ok(frame)
    }

    /// The host had `received` bytes at `at`. Each part that arrived tells a round trip, and the
    /// bytes that arrived in the last round trip on the host's clock tell the speed.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a speed does not need exact byte counts"
    )]
    fn received(&mut self, received: u64, at: u64, now: tokio::time::Instant) -> Result<()> {
        if received < self.received || received > self.sent {
            return Err(invalid(
                "The host confirmed bytes of the picture that were not sent.",
            ));
        }
        self.received = received;
        while let Some(&(end, left)) = self.parts.front()
            && end <= received
        {
            self.parts.pop_front();
            let trip = now.saturating_duration_since(left);
            if !trip.is_zero() {
                self.round_trip = self.round_trip.min(trip);
            }
        }
        let span = u64::try_from(self.round_trip.as_micros()).unwrap_or(u64::MAX);
        while self
            .arrivals
            .get(1)
            .is_some_and(|&(then, _)| then.saturating_add(span) <= at)
        {
            self.arrivals.pop_front();
        }
        if let Some(&(then, before)) = self.arrivals.front()
            && then.saturating_add(span) <= at
            && then < at
        {
            self.speed = self
                .speed
                .max((received - before) as f64 * 1e6 / (at - then) as f64);
        }
        self.arrivals.push_back((at, received));
        Ok(())
    }
}

struct Pending {
    reply: oneshot::Sender<Result<Value>>,
    deadline: tokio::time::Instant,
    closes: bool,
}

pub(super) async fn connect(network: BackendClient, id: Uuid) -> Result<RemoteConnection> {
    let credentials = network.credentials()?;
    if !credentials.enrolled {
        return Err(Error::EnrollmentRequired);
    }
    let (mut dto, kept) = network.known_session(&credentials, id).await?;
    let (channel, public) = match open(&network, &credentials, &dto).await {
        Err(
            Error::Stale
            | Error::Backend {
                status: 404 | 409, ..
            },
        ) if kept => {
            dto = network.fetch_session(&credentials, id).await?;
            open(&network, &credentials, &dto).await?
        }
        opened => opened?,
    };
    let (updates_tx, updates) = mpsc::channel(128);
    let (commands, commands_rx) = mpsc::channel(128);
    network.check_credentials(&credentials)?;
    let cancellation = credentials.cancel.child_token();
    {
        let mut connections = network.inner.connections.lock().await;
        connections.retain(|token| !token.is_cancelled());
        connections.push(cancellation.clone());
    }
    let session = dto.clone().into();
    let cancel = cancellation.clone();
    let host = Arc::new(direct::Host::new(
        network.channel_keys(&credentials)?,
        dto.host_device_id.clone(),
        public.clone(),
    ));
    tokio::spawn(async move {
        let mut viewer = Viewer::new(channel, commands_rx, updates_tx.clone(), host);
        let result = tokio::select! {
            () = cancel.cancelled() => Ok(()),
            () = network.inner.shutdown.cancelled() => Ok(()),
            result = viewer.run(&network, &credentials, &dto, &public) => result,
        };
        if let Err(error) = &result {
            viewer.settle(error);
        }
        if !cancel.is_cancelled() {
            let reason = result.err().map_or_else(
                || text!("The connection to the terminal ended."),
                |error| error.text(),
            );
            let _outcome = updates_tx.try_send(RemoteUpdate::Closed {
                reason: viewer.unconfirmed(reason),
            });
        }
        cancel.cancel();
        viewer.channel.close().await;
    });
    Ok(RemoteConnection {
        session,
        updates,
        commands,
        cancellation,
    })
}

async fn open(
    network: &BackendClient,
    credentials: &Credentials,
    dto: &SessionDto,
) -> Result<(Channel<Pipe>, Vec<u8>)> {
    let pipe = network.inner.link.open(dto.id, dto.incarnation_id).await?;
    let host_key = |owner: &crate::identity::pins::VerifiedIdentity| {
        owner
            .devices
            .get(&dto.host_device_id)
            .map(|certificate| certificate.sig_public_key.clone())
    };
    let known = if let Some(room) = dto
        .mission_id
        .filter(|_| dto.owner_user_id != credentials.user_id)
    {
        network
            .room_identity(credentials, room, &dto.owner_user_id)
            .await?
    } else {
        network
            .known_identity(credentials, &dto.owner_user_id)
            .await?
    };
    let public = match host_key(&known) {
        Some(public) => public,
        None => host_key(
            &network
                .fetch_identity_with(credentials, &dto.owner_user_id)
                .await?,
        )
        .ok_or_else(|| Error::Trust(text!("The hosting device is not approved.")))?,
    };
    let mut channel = Channel::connect(
        pipe,
        &*network.channel_keys(credentials)?,
        &dto.host_device_id,
        &public,
    )
    .await?;
    let version = channel.version()?;
    match tokio::time::timeout(Duration::from_secs(20), channel.receive())
        .await
        .map_err(|_| Error::Closed)??
    {
        Some((Frame::Accept(accept), _)) => {
            if accept.protocol_version != version {
                return Err(Error::ChannelVersion {
                    peer_newer: accept.protocol_version > version,
                });
            }
            if (
                accept.session_id,
                accept.incarnation_id,
                accept.host_device_id.as_str(),
            ) != (dto.id, dto.incarnation_id, dto.host_device_id.as_str())
            {
                return Err(untrusted("The host answered for another terminal."));
            }
            Ok((channel, public))
        }
        Some((Frame::Refuse(refuse), _)) => Err(match refuse.code.as_str() {
            "busy" => Error::Busy,
            "access" => Error::Backend {
                status: 403,
                message: refuse.message,
            },
            _ => Error::Text(refuse.message),
        }),
        _ => Err(Error::Closed),
    }
}

async fn host_trusted(
    network: &BackendClient,
    credentials: &Credentials,
    dto: &SessionDto,
    public: &[u8],
) -> Result<()> {
    let current_credentials = network.credentials()?;
    network.check_credentials(credentials)?;
    let current: SessionDto = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{}", dto.id),
            &current_credentials,
            None,
        )
        .await?;
    if current.incarnation_id != dto.incarnation_id {
        return Err(Error::Stale);
    }
    let owner = if let Some(room) = dto
        .mission_id
        .filter(|_| dto.owner_user_id != credentials.user_id)
    {
        network
            .room_identity(&current_credentials, room, &dto.owner_user_id)
            .await?
    } else {
        network
            .fetch_identity_with(&current_credentials, &dto.owner_user_id)
            .await?
    };
    if owner
        .devices
        .get(&dto.host_device_id)
        .is_none_or(|certificate| certificate.sig_public_key != public)
    {
        return Err(Error::Trust(text!(
            "The hosting device is no longer trusted."
        )));
    }
    Ok(())
}

struct PipeOut {
    frames: mpsc::Sender<Vec<u8>>,
    filling: Vec<u8>,
}

struct Viewer<T> {
    channel: Channel<T>,
    commands: mpsc::Receiver<RemoteRequest>,
    updates: mpsc::Sender<RemoteUpdate>,
    pending: BTreeMap<Uuid, Pending>,
    snapshot: Vec<u8>,
    snapshot_sequence: Option<u64>,
    started: bool,
    ended: bool,
    received: u64,
    acknowledged: u64,
    acknowledge_at: Option<tokio::time::Instant>,
    heartbeat: u32,
    heard: tokio::time::Instant,
    input_sent: u64,
    input_confirmed: u64,
    pipes: BTreeMap<Uuid, PipeOut>,
    host: Arc<direct::Host>,
    asking: BTreeMap<Uuid, oneshot::Sender<(Arc<direct::Host>, Vec<std::net::SocketAddr>)>>,
    upload: Option<Upload>,
}

impl<T: Transport> Viewer<T> {
    fn new(
        channel: Channel<T>,
        commands: mpsc::Receiver<RemoteRequest>,
        updates: mpsc::Sender<RemoteUpdate>,
        host: Arc<direct::Host>,
    ) -> Self {
        Self {
            channel,
            commands,
            updates,
            pending: BTreeMap::new(),
            snapshot: Vec::new(),
            snapshot_sequence: None,
            started: false,
            ended: false,
            received: 0,
            acknowledged: 0,
            acknowledge_at: None,
            heartbeat: 0,
            heard: tokio::time::Instant::now(),
            input_sent: 0,
            input_confirmed: 0,
            pipes: BTreeMap::new(),
            host,
            asking: BTreeMap::new(),
            upload: None,
        }
    }

    fn unconfirmed(&self, reason: Text) -> Text {
        match self.input_sent - self.input_confirmed {
            0 => reason,
            _ => text!(
                "{reason} Kodosi did not send some of your last input.",
                reason = reason
            ),
        }
    }

    /// A close that waits when its terminal is gone did its work. Its answer can lose the race
    /// with the end of the terminal, and the service drops the pipe of a terminal that is gone.
    fn settle(&mut self, error: &Error) {
        if !self.ended && !matches!(error, Error::Backend { status: 404, .. }) {
            return;
        }
        for (_, pending) in std::mem::take(&mut self.pending) {
            if pending.closes {
                drop(pending.reply.send(Ok(Value::Null)));
            }
        }
    }

    async fn run(
        &mut self,
        network: &BackendClient,
        credentials: &Credentials,
        dto: &SessionDto,
        public: &[u8],
    ) -> Result<()> {
        let generation = credentials.generation;
        let mut deadlines = tokio::time::interval(Duration::from_millis(250));
        deadlines.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut trust = tokio::time::interval(TRUST_CHECK);
        trust.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        trust.tick().await;
        loop {
            check_generation(network, generation)?;
            if self.step(&mut deadlines, &mut trust).await? {
                match host_trusted(network, credentials, dto, public).await {
                    Err(error) if error.unanswered() => {
                        tracing::warn!(%error, "host trust check got no answer; it will be tried again");
                    }
                    result => result?,
                }
            }
            if self.updates.is_closed() {
                return Ok(());
            }
        }
    }

    async fn step(
        &mut self,
        deadlines: &mut tokio::time::Interval,
        trust: &mut tokio::time::Interval,
    ) -> Result<bool> {
        let acknowledge = self.acknowledge_at;
        tokio::select! {
            _ = trust.tick() => return Ok(true),
            _ = deadlines.tick() => {
                let now = tokio::time::Instant::now();
                if self.heard.elapsed() >= CHANNEL_SILENCE {
                    return Err(Error::Text(text!("The host did not answer.")));
                }
                let expired = self.pending.iter().filter(|(_, pending)| pending.deadline <= now).map(|(id, _)| *id).collect::<Vec<_>>();
                for id in expired {
                    if let Some(pending) = self.pending.remove(&id) {
                        drop(pending.reply.send(Err(Error::Text(text!("The host did not confirm the operation, and Kodosi did not send it again.")))));
                    }
                }
                if let Some(upload) = self.upload.take_if(|upload| upload.deadline <= now) {
                    drop(upload.reply.send(Err(Error::Text(picture_not_confirmed()))));
                }
            }
            () = std::future::ready(()), if self.upload.as_ref().is_some_and(Upload::wants_part) => self.send_part().await?,
            () = async { tokio::time::sleep_until(acknowledge.unwrap_or_else(tokio::time::Instant::now)).await }, if acknowledge.is_some() => {
                self.acknowledge().await?;
            }
            request = self.commands.recv(), if self.input_sent - self.input_confirmed < UNCONFIRMED_INPUT => match request {
                Some(request) => self.request(request).await?,
                None => return Err(Error::Closed),
            },
            frame = self.channel.receive() => match frame? {
                Some((frame, size)) => self.frame(frame, size as u64).await?,
                None => return Err(Error::Closed),
            },
        }
        Ok(false)
    }

    async fn acknowledge(&mut self) -> Result<()> {
        self.acknowledge_at = None;
        if self.received != self.acknowledged {
            self.acknowledged = self.received;
            self.channel
                .send(&Frame::Ack {
                    received: self.received,
                })
                .await?;
        }
        Ok(())
    }

    async fn request(&mut self, request: RemoteRequest) -> Result<()> {
        match request {
            RemoteRequest::Checkpoint => {
                if self.started {
                    self.channel.send(&Frame::Refresh).await?;
                }
            }
            RemoteRequest::Control {
                control: TerminalControl::Input { bytes },
                reply,
            } => {
                if !self.started {
                    drop(reply.send(Err(Error::Text(text!(
                        "Wait for the terminal snapshot before sending input."
                    )))));
                    return Ok(());
                }
                if bytes.is_empty() {
                    drop(reply.send(Ok(Value::Null)));
                    return Ok(());
                }
                let length = bytes.len() as u64;
                let frame = Frame::Input {
                    offset: self.input_sent,
                    heartbeat: self.heartbeat,
                    bytes: bytes.into(),
                };
                if let Err(error) = frame.encode() {
                    drop(reply.send(Err(error)));
                    return Ok(());
                }
                if let Err(error) = self.channel.send(&frame).await {
                    drop(reply.send(Err(invalid(error.to_string()))));
                    return Err(error);
                }
                self.input_sent += length;
                drop(reply.send(Ok(Value::Null)));
            }
            RemoteRequest::Picture { picture, reply } => self.offer(picture, reply).await?,
            RemoteRequest::Control { control, reply } => {
                if !self.started {
                    drop(reply.send(Err(Error::Text(text!(
                        "Wait for the terminal snapshot before sending input."
                    )))));
                    return Ok(());
                }
                if self.pending.len() >= 128 {
                    drop(reply.send(Err(Error::Busy)));
                    return Ok(());
                }
                let request_id = Uuid::now_v7();
                let closes = matches!(control, TerminalControl::Close);
                let frame = Frame::Control {
                    request_id,
                    control,
                };
                match frame.encode() {
                    Ok(_) => {}
                    Err(error) => {
                        drop(reply.send(Err(error)));
                        return Ok(());
                    }
                }
                self.channel.send(&frame).await?;
                self.pending.insert(
                    request_id,
                    Pending {
                        reply,
                        deadline: tokio::time::Instant::now() + CONTROL_TIMEOUT,
                        closes,
                    },
                );
            }
            pipe => self.pipe_request(pipe).await?,
        }
        Ok(())
    }

    async fn pipe_request(&mut self, request: RemoteRequest) -> Result<()> {
        match request {
            RemoteRequest::PipeOpen {
                pipe,
                key,
                name,
                frames,
            } => {
                self.pipes.insert(
                    pipe,
                    PipeOut {
                        frames,
                        filling: Vec::new(),
                    },
                );
                self.channel
                    .send(&Frame::PipeOpen { pipe, key, name })
                    .await?;
            }
            RemoteRequest::PipeData { pipe, more, part } => {
                self.channel
                    .send(&Frame::PipeData { pipe, more, part })
                    .await?;
            }
            RemoteRequest::PipeClose { pipe } => {
                self.pipes.remove(&pipe);
                self.channel.send(&Frame::PipeClose { pipe }).await?;
            }
            RemoteRequest::DirectAsk { pipe, reply } => {
                self.asking.retain(|_, waiting| !waiting.is_closed());
                self.asking.insert(pipe, reply);
                self.channel.send(&Frame::DirectAsk { pipe }).await?;
            }
            RemoteRequest::Control { .. }
            | RemoteRequest::Picture { .. }
            | RemoteRequest::Checkpoint => {}
        }
        Ok(())
    }

    /// A slow view loses its pipe and joins again, so the terminal never waits for it.
    async fn pipe_part(&mut self, pipe: Uuid, more: bool, part: &[u8]) -> Result<()> {
        let Some(open) = self.pipes.get_mut(&pipe) else {
            return Ok(());
        };
        if open.filling.len() + part.len() > crate::canvas::MOST_PIPE_FRAME {
            return Err(invalid("A document frame exceeds its bound."));
        }
        open.filling.extend_from_slice(part);
        if !more
            && open
                .frames
                .try_send(std::mem::take(&mut open.filling))
                .is_err()
        {
            self.pipes.remove(&pipe);
            self.channel.send(&Frame::PipeClose { pipe }).await?;
        }
        Ok(())
    }

    #[expect(clippy::too_many_lines, reason = "one match reads every host frame")]
    async fn frame(&mut self, frame: Frame, size: u64) -> Result<()> {
        self.heard = tokio::time::Instant::now();
        if frame.carries_output() {
            self.received += size;
            if self.received - self.acknowledged >= ACKNOWLEDGE_BYTES {
                self.acknowledge().await?;
            } else if self.acknowledge_at.is_none() {
                self.acknowledge_at = Some(tokio::time::Instant::now() + ACKNOWLEDGE_DELAY);
            }
        }
        match frame {
            Frame::Keyframe {
                next_sequence,
                more,
                part,
            } => {
                if self.snapshot_sequence != Some(next_sequence) {
                    self.snapshot.clear();
                    self.snapshot_sequence = Some(next_sequence);
                }
                if self.snapshot.len() + part.len() > wire::PACKED_SNAPSHOT_LIMIT {
                    return Err(invalid("The terminal snapshot exceeds its bound."));
                }
                self.snapshot.extend_from_slice(&part);
                if more {
                    return Ok(());
                }
                let checkpoint = wire::snapshot(&std::mem::take(&mut self.snapshot))?;
                self.snapshot_sequence = None;
                self.acknowledge().await?;
                if std::mem::replace(&mut self.started, true) {
                    self.update(RemoteUpdate::Resync).await?;
                }
                self.update(RemoteUpdate::Checkpoint {
                    checkpoint,
                    next_sequence,
                })
                .await?;
            }
            Frame::Output {
                first_sequence,
                chunks,
            } => {
                for (sequence, bytes) in (first_sequence..).zip(chunks) {
                    self.update(RemoteUpdate::Raw { sequence, bytes }).await?;
                }
            }
            Frame::Resize {
                rows,
                cols,
                cell,
                at_sequence,
            } => {
                self.update(RemoteUpdate::Resize {
                    rows,
                    cols,
                    cell,
                    at_sequence,
                })
                .await?;
            }
            Frame::Metadata(metadata) => self.update(RemoteUpdate::Metadata(metadata)).await?,
            Frame::Heartbeat { number, .. } => {
                self.heartbeat = number;
                self.acknowledge_at = None;
                self.acknowledged = self.received;
                self.channel
                    .send(&Frame::Ack {
                        received: self.received,
                    })
                    .await?;
            }
            Frame::End(_) => {
                self.ended = true;
                self.input_confirmed = self.input_sent;
                self.update(RemoteUpdate::Ended).await?;
                return Err(Error::Closed);
            }
            Frame::InputAck { offset } => {
                if offset < self.input_confirmed || offset > self.input_sent {
                    return Err(invalid("The host confirmed input that was not sent."));
                }
                self.input_confirmed = offset;
            }
            Frame::ControlResult(ControlResult {
                request_id,
                accepted,
                message,
            }) => {
                if let Some(pending) = self.pending.remove(&request_id) {
                    drop(pending.reply.send(if accepted {
                        Ok(Value::Null)
                    } else {
                        Err(message.map_or_else(
                            || Error::Text(text!("The terminal refused the control.")),
                            Error::Text,
                        ))
                    }));
                }
            }
            Frame::ControlValue { request_id, value } => {
                if let Some(pending) = self.pending.remove(&request_id) {
                    drop(pending.reply.send(Ok(value)));
                }
            }
            Frame::PipeData { pipe, more, part } => self.pipe_part(pipe, more, &part).await?,
            Frame::PipeClose { pipe } => {
                self.pipes.remove(&pipe);
            }
            Frame::DirectOffer { pipe, addresses } => {
                if let Some(waiting) = self.asking.remove(&pipe) {
                    drop(waiting.send((Arc::clone(&self.host), addresses)));
                }
            }
            Frame::PictureState(state) => self.picture_state(state).await?,
            _ => return Err(invalid("Unsupported terminal host frame.")),
        }
        Ok(())
    }

    async fn offer(
        &mut self,
        picture: crate::terminal::Picture,
        reply: oneshot::Sender<Result<()>>,
    ) -> Result<()> {
        if !self.started {
            drop(reply.send(Err(Error::Text(text!(
                "Wait for the terminal snapshot before sending a picture."
            )))));
            return Ok(());
        }
        if self.upload.is_some() {
            drop(reply.send(Err(Error::Busy)));
            return Ok(());
        }
        let id = Uuid::now_v7();
        self.channel
            .send(&Frame::Picture(PictureOffer {
                id,
                size: picture.bytes.len() as u64,
                sha256: picture.sha256(),
                format: picture.format,
            }))
            .await?;
        self.upload = Some(Upload::new(id, picture, reply, tokio::time::Instant::now()));
        Ok(())
    }

    async fn send_part(&mut self) -> Result<()> {
        let Some(upload) = self.upload.as_mut() else {
            return Ok(());
        };
        let frame = upload.part(tokio::time::Instant::now())?;
        self.channel.send(&frame).await.map(|_| ())
    }

    /// Follows the host's answer for the picture on its way. A late answer, for a picture that
    /// this view stopped waiting for, changes nothing.
    async fn picture_state(&mut self, state: PictureState) -> Result<()> {
        let Some(upload) = self.upload.as_mut().filter(|upload| upload.id == state.id) else {
            return Ok(());
        };
        let now = tokio::time::Instant::now();
        let total = upload.picture.bytes.len() as u64;
        match state.step {
            PictureStep::Receiving => {
                upload.received(state.received, state.at, now)?;
                upload.asked = true;
                upload.deadline = now + CONTROL_TIMEOUT;
                if upload.told + PROGRESS_SPACING <= now || state.received == total {
                    upload.told = now;
                    self.update(RemoteUpdate::Picture {
                        sent: state.received,
                        total,
                    })
                    .await?;
                }
            }
            PictureStep::Pasted => {
                if let Some(upload) = self.upload.take() {
                    drop(upload.reply.send(Ok(())));
                }
            }
            PictureStep::Refused => {
                if let Some(upload) = self.upload.take() {
                    drop(upload.reply.send(Err(state.message.map_or_else(
                        || Error::Text(text!("The other computer refused the picture.")),
                        Error::Text,
                    ))));
                }
            }
        }
        Ok(())
    }

    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "exclusive borrow keeps the viewer future Send across the await"
    )]
    async fn update(&mut self, update: RemoteUpdate) -> Result<()> {
        self.updates.send(update).await.map_err(|_| Error::Closed)
    }
}

#[cfg(test)]
mod tests;
