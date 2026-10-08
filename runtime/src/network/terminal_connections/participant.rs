use super::{
    channel::{Channel, Transport},
    link::Pipe,
    wire::{ControlResult, Frame},
    *,
};

const ACKNOWLEDGE_BYTES: u64 = 4 * 1024;
const ACKNOWLEDGE_DELAY: Duration = Duration::from_millis(100);
const UNCONFIRMED_INPUT: u64 = 64 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(12);
const SILENCE: Duration = Duration::from_secs(45);

struct Pending {
    reply: oneshot::Sender<Result<Value>>,
    deadline: tokio::time::Instant,
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
    tokio::spawn(async move {
        let mut viewer = Viewer::new(channel, commands_rx, updates_tx.clone());
        let result = tokio::select! {
            () = cancel.cancelled() => Ok(()),
            () = network.inner.shutdown.cancelled() => Ok(()),
            result = viewer.run(&network, &credentials, &dto, &public) => result,
        };
        if !cancel.is_cancelled() {
            let reason = result.err().map_or_else(
                || "Remote terminal disconnected.".to_owned(),
                |error| error.to_string(),
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
    let pipe = network.inner.link.open(dto.id, dto.incarnation_id)?;
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
        .ok_or_else(|| Error::Trust("The hosting device is not approved.".into()))?,
    };
    let mut channel = Channel::connect(
        pipe,
        &*network.channel_keys(credentials)?,
        &dto.host_device_id,
        &public,
    )
    .await?;
    match tokio::time::timeout(Duration::from_secs(20), channel.receive())
        .await
        .map_err(|_| Error::Closed)??
    {
        Some((Frame::Accept(accept), _)) => {
            if accept.protocol_version != crate::protocol::TERMINAL_CONNECTION_VERSION
                || accept.session_id != dto.id
                || accept.incarnation_id != dto.incarnation_id
                || accept.host_device_id != dto.host_device_id
            {
                return Err(Error::Trust(
                    "The host answered for another terminal.".into(),
                ));
            }
            Ok((channel, public))
        }
        Some((Frame::Refuse(refuse), _)) => Err(match refuse.code.as_str() {
            "busy" => Error::Busy,
            "gone" => Error::Stale,
            "access" => Error::Backend {
                status: 403,
                message: refuse.message,
            },
            _ => invalid(refuse.message),
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
        return Err(Error::Trust(
            "The hosting device is no longer trusted.".into(),
        ));
    }
    Ok(())
}

struct Viewer<T> {
    channel: Channel<T>,
    commands: mpsc::Receiver<RemoteRequest>,
    updates: mpsc::Sender<RemoteUpdate>,
    pending: BTreeMap<Uuid, Pending>,
    snapshot: Vec<u8>,
    snapshot_sequence: Option<u64>,
    started: bool,
    received: u64,
    acknowledged: u64,
    acknowledge_at: Option<tokio::time::Instant>,
    heartbeat: u32,
    heard: tokio::time::Instant,
    input_sent: u64,
    input_confirmed: u64,
}

impl<T: Transport> Viewer<T> {
    fn new(
        channel: Channel<T>,
        commands: mpsc::Receiver<RemoteRequest>,
        updates: mpsc::Sender<RemoteUpdate>,
    ) -> Self {
        Self {
            channel,
            commands,
            updates,
            pending: BTreeMap::new(),
            snapshot: Vec::new(),
            snapshot_sequence: None,
            started: false,
            received: 0,
            acknowledged: 0,
            acknowledge_at: None,
            heartbeat: 0,
            heard: tokio::time::Instant::now(),
            input_sent: 0,
            input_confirmed: 0,
        }
    }

    fn unconfirmed(&self, reason: String) -> String {
        match self.input_sent - self.input_confirmed {
            0 => reason,
            _ => format!("{reason} {INPUT_NOT_SENT}"),
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
                if self.heard.elapsed() >= SILENCE {
                    return Err(invalid("The host did not answer."));
                }
                let expired = self.pending.iter().filter(|(_, pending)| pending.deadline <= now).map(|(id, _)| *id).collect::<Vec<_>>();
                for id in expired {
                    if let Some(pending) = self.pending.remove(&id) {
                        drop(pending.reply.send(Err(invalid("The host did not confirm the operation; it was not retried."))));
                    }
                }
            }
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
                    drop(reply.send(Err(invalid(
                        "Wait for the terminal snapshot before sending input.",
                    ))));
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
            RemoteRequest::Control { control, reply } => {
                if !self.started {
                    drop(reply.send(Err(invalid(
                        "Wait for the terminal snapshot before sending input.",
                    ))));
                    return Ok(());
                }
                if self.pending.len() >= 128 {
                    drop(reply.send(Err(Error::Busy)));
                    return Ok(());
                }
                let request_id = Uuid::now_v7();
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
                    },
                );
            }
        }
        Ok(())
    }

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
                    fresh: true,
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
                at_sequence,
            } => {
                self.update(RemoteUpdate::Resize {
                    rows,
                    cols,
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
            Frame::End(end) => {
                self.input_confirmed = self.input_sent;
                self.update(RemoteUpdate::Ended {
                    final_sequence: end.final_sequence,
                })
                .await?;
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
                        Err(invalid(message))
                    }));
                }
            }
            _ => return Err(invalid("Unsupported terminal host frame.")),
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
