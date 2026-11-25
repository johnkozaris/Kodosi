use std::{collections::VecDeque, net::SocketAddr, sync::atomic::AtomicUsize};

use tokio::sync::watch;

use super::{
    channel::Transport,
    peers::{Lane, Media},
    wire::Frame,
    *,
};

#[cfg(test)]
pub(super) mod tests;

const DATA: u8 = 0;
const ACK: u8 = 1;
const RESUME: u8 = 2;
const SENDING: usize = 64;
const UNACKNOWLEDGED_BYTES: usize = 24 * 1024 * 1024;
const RECEIVING_BYTES: usize = 16 * 1024 * 1024;
const ACK_AFTER: Duration = Duration::from_millis(200);
const ACK_COUNT: u64 = 32;
const STALL: Duration = Duration::from_secs(3);
/// Delay before flagging unacknowledged traffic as late (about when typing lag is noticed).
const LATE: Duration = Duration::from_secs(1);
const RESUME_WAIT: Duration = Duration::from_secs(10);
const ANSWER_WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Path {
    Relay,
    Direct,
}

/// Only the viewer initiates promotion to a direct lane; either side may fall back to the relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    Viewer,
    #[cfg_attr(
        not(any(feature = "host", test)),
        expect(dead_code, reason = "only a host answers")
    )]
    Host,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Carrier {
    Relay,
    Lane,
}

enum Incoming {
    Data(Vec<u8>),
    Closed(Option<Error>),
}

/// Reliable, ordered message transport over a relay pipe with seamless failover to/from a direct
/// QUIC lane. The relay pipe remains the channel anchor; every message carries a sequence number
/// and cumulative ack so unacknowledged messages replay across path switches without loss,
/// duplication, or reordering.
pub(super) struct Route {
    outgoing: mpsc::Sender<Vec<u8>>,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    waiting: Arc<AtomicUsize>,
}

#[derive(Clone)]
pub(super) struct Paths {
    lanes: mpsc::Sender<Lane>,
    path: watch::Receiver<Path>,
    link: watch::Receiver<Option<Media>>,
    late: watch::Receiver<bool>,
}

impl Route {
    pub(super) fn new<R: Transport + 'static>(relay: R, role: Role) -> (Self, Paths) {
        let (outgoing, sent) = mpsc::channel(SENDING);
        let (delivered, incoming) = mpsc::unbounded_channel();
        let (lanes, attached) = mpsc::channel(1);
        let (moved, path) = watch::channel(Path::Relay);
        let (linked, link) = watch::channel(None);
        let (lateness, late) = watch::channel(false);
        let waiting = Arc::new(AtomicUsize::new(0));
        let now = tokio::time::Instant::now();
        tokio::spawn(
            Actor {
                relay,
                lane: None,
                lane_lost: false,
                attached,
                role,
                sending: Carrier::Relay,
                proposed: None,
                proposed_at: now,
                held: false,
                closing: None,
                next: 1,
                unacknowledged: VecDeque::new(),
                unacknowledged_bytes: 0,
                progress: now,
                delivered: 0,
                told: 0,
                ack_at: None,
                outgoing: sent,
                incoming: delivered,
                waiting: Arc::clone(&waiting),
                path: moved,
                link: linked,
                late: lateness,
            }
            .run(),
        );
        (
            Self {
                outgoing,
                incoming,
                waiting,
            },
            Paths {
                lanes,
                path,
                link,
                late,
            },
        )
    }
}

impl Transport for Route {
    async fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.outgoing.send(bytes).await.map_err(|_| Error::Closed)
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        match self.incoming.recv().await {
            Some(Incoming::Data(bytes)) => {
                self.waiting.fetch_sub(bytes.len(), Ordering::AcqRel);
                Ok(Some(bytes))
            }
            Some(Incoming::Closed(Some(error))) => Err(error),
            Some(Incoming::Closed(None)) | None => Ok(None),
        }
    }
}

impl Paths {
    pub(super) fn attach(&self, lane: Lane) -> bool {
        self.lanes.try_send(lane).is_ok()
    }

    pub(super) fn late(&self) -> watch::Receiver<bool> {
        self.late.clone()
    }

    pub(super) fn path(&self) -> watch::Receiver<Path> {
        self.path.clone()
    }

    /// The QUIC connection under the lane while the route sends on it, for video datagrams.
    pub(super) fn link(&self) -> watch::Receiver<Option<Media>> {
        self.link.clone()
    }

    async fn went_direct(&mut self) -> bool {
        tokio::time::timeout(STALL * 2, self.path.wait_for(|path| *path != Path::Relay))
            .await
            .is_ok_and(|seen| seen.is_ok())
    }

    async fn back_on_relay(&mut self) -> bool {
        self.path
            .wait_for(|path| *path == Path::Relay)
            .await
            .is_ok()
    }
}

struct Actor<R> {
    relay: R,
    lane: Option<Lane>,
    lane_lost: bool,
    attached: mpsc::Receiver<Lane>,
    role: Role,
    sending: Carrier,
    proposed: Option<Carrier>,
    proposed_at: tokio::time::Instant,
    held: bool,
    closing: Option<tokio::time::Instant>,
    next: u64,
    unacknowledged: VecDeque<(u64, Vec<u8>)>,
    unacknowledged_bytes: usize,
    progress: tokio::time::Instant,
    delivered: u64,
    told: u64,
    ack_at: Option<tokio::time::Instant>,
    outgoing: mpsc::Receiver<Vec<u8>>,
    incoming: mpsc::UnboundedSender<Incoming>,
    waiting: Arc<AtomicUsize>,
    path: watch::Sender<Path>,
    link: watch::Sender<Option<Media>>,
    late: watch::Sender<bool>,
}

impl<R: Transport> Actor<R> {
    async fn run(mut self) {
        let ended = self.serve().await;
        drop(self.incoming.send(Incoming::Closed(ended.err())));
    }

    async fn serve(&mut self) -> Result<()> {
        let timer = tokio::time::sleep_until(tokio::time::Instant::now());
        tokio::pin!(timer);
        loop {
            let due = self.due();
            if let Some(at) = due
                && timer.deadline() != at
            {
                timer.as_mut().reset(at);
            }
            tokio::select! {
                biased;
                Some(lane) = self.attached.recv() => self.attach(lane).await?,
                message = self.relay.receive() => match message? {
                    Some(message) => self.heard(Carrier::Relay, &message).await?,
                    None => return Ok(()),
                },
                message = next_on(&mut self.lane) => match message {
                    Some(message) => self.heard(Carrier::Lane, &message).await?,
                    None => self.lane_lost = true,
                },
                payload = self.outgoing.recv(), if !self.held && self.closing.is_none() => match payload {
                    Some(payload) => self.send(payload).await?,
                    None if self.delivers_on_lane() => self.closing = Some(tokio::time::Instant::now()),
                    None => {
                        self.ack().await?;
                        return Ok(());
                    }
                },
                () = &mut timer, if due.is_some() => self.timers().await?,
            }
            if std::mem::take(&mut self.lane_lost) {
                self.lost().await?;
            }
            if self.closing.is_some() && self.unacknowledged.is_empty() {
                return Ok(());
            }
            self.tell_late();
        }
    }

    async fn attach(&mut self, lane: Lane) -> Result<()> {
        if self.sending == Carrier::Lane || self.proposed.is_some() {
            return Ok(());
        }
        self.lane = Some(lane);
        if self.role == Role::Viewer {
            self.propose(Carrier::Lane).await?;
        }
        Ok(())
    }

    async fn propose(&mut self, carrier: Carrier) -> Result<()> {
        self.proposed = Some(carrier);
        self.proposed_at = tokio::time::Instant::now();
        self.write(carrier, numbered(RESUME, self.delivered))
            .await?;
        self.told = self.delivered;
        Ok(())
    }

    async fn heard(&mut self, from: Carrier, message: &[u8]) -> Result<()> {
        let Some((&kind, rest)) = message.split_first() else {
            return Err(broken());
        };
        match kind {
            DATA => {
                let (sequence, acknowledged, payload) = data_parts(rest).ok_or_else(broken)?;
                self.acknowledge(acknowledged);
                if sequence == self.delivered + 1 {
                    self.delivered = sequence;
                    self.deliver(payload.to_vec())?;
                    if self.delivered - self.told >= ACK_COUNT {
                        self.ack().await?;
                    } else {
                        self.ack_at
                            .get_or_insert_with(|| tokio::time::Instant::now() + ACK_AFTER);
                    }
                } else if sequence > self.delivered + 1 {
                    return Err(broken());
                }
            }
            ACK => self.acknowledge(number(rest)?),
            RESUME => self.resumed(from, number(rest)?).await?,
            _ => return Err(broken()),
        }
        Ok(())
    }

    async fn resumed(&mut self, from: Carrier, received: u64) -> Result<()> {
        self.acknowledge(received);
        if self.proposed != Some(from) {
            self.write(from, numbered(RESUME, self.delivered)).await?;
            self.told = self.delivered;
        }
        let again = self
            .unacknowledged
            .iter()
            .map(|(sequence, payload)| data(*sequence, self.delivered, payload))
            .collect::<Vec<_>>();
        for message in again {
            self.write(from, message).await?;
        }
        self.told = self.delivered;
        self.ack_at = None;
        self.sending = from;
        self.proposed = None;
        self.held = false;
        self.progress = tokio::time::Instant::now();
        if from == Carrier::Relay
            && let Some(lane) = self.lane.take()
        {
            lane.abandon();
        }
        self.publish();
        Ok(())
    }

    async fn lost(&mut self) -> Result<()> {
        self.lane = None;
        let on_lane = self.sending == Carrier::Lane || self.proposed == Some(Carrier::Lane);
        if on_lane && self.proposed != Some(Carrier::Relay) {
            self.held = true;
            self.propose(Carrier::Relay).await?;
        }
        self.publish();
        Ok(())
    }

    async fn send(&mut self, payload: Vec<u8>) -> Result<()> {
        let sequence = self.next;
        self.next += 1;
        if self.unacknowledged.is_empty() {
            self.progress = tokio::time::Instant::now();
        }
        self.unacknowledged_bytes += payload.len();
        if self.unacknowledged_bytes > UNACKNOWLEDGED_BYTES {
            return Err(Error::Closed);
        }
        let message = data(sequence, self.delivered, &payload);
        self.unacknowledged.push_back((sequence, payload));
        self.told = self.delivered;
        self.ack_at = None;
        self.write(self.sending, message).await
    }

    async fn write(&mut self, to: Carrier, message: Vec<u8>) -> Result<()> {
        match to {
            Carrier::Relay => self.relay.send(message).await,
            Carrier::Lane => {
                let written = match self.lane.as_mut() {
                    Some(lane) => tokio::time::timeout(STALL, lane.write(&message)).await,
                    None => Ok(Err(Error::Closed)),
                };
                match written {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => {
                        self.lane = None;
                        self.lane_lost = true;
                    }
                    Err(_) => self.abandon_lane(),
                }
                Ok(())
            }
        }
    }

    fn abandon_lane(&mut self) {
        if let Some(lane) = self.lane.take() {
            lane.abandon();
        }
        self.lane_lost = true;
    }

    fn acknowledge(&mut self, upto: u64) {
        let mut advanced = false;
        while let Some((_, payload)) = self
            .unacknowledged
            .pop_front_if(|(sequence, _)| *sequence <= upto)
        {
            self.unacknowledged_bytes -= payload.len();
            advanced = true;
        }
        if advanced {
            self.progress = tokio::time::Instant::now();
        }
    }

    /// Relay pipes flush in-flight messages on close, but direct lanes may drop them, so closing
    /// while using a lane waits for pending messages to be acknowledged.
    fn delivers_on_lane(&self) -> bool {
        !self.unacknowledged.is_empty()
            && (self.held || self.sending == Carrier::Lane || self.proposed == Some(Carrier::Lane))
    }

    fn deliver(&self, payload: Vec<u8>) -> Result<()> {
        if self.closing.is_some() {
            return Ok(());
        }
        if self.waiting.fetch_add(payload.len(), Ordering::AcqRel) + payload.len() > RECEIVING_BYTES
        {
            return Err(Error::Closed);
        }
        self.incoming
            .send(Incoming::Data(payload))
            .map_err(|_| Error::Closed)
    }

    async fn ack(&mut self) -> Result<()> {
        self.ack_at = None;
        if self.told < self.delivered {
            let to = if self.held {
                Carrier::Relay
            } else {
                self.sending
            };
            self.write(to, numbered(ACK, self.delivered)).await?;
            self.told = self.delivered;
        }
        Ok(())
    }

    async fn timers(&mut self) -> Result<()> {
        let now = tokio::time::Instant::now();
        if self.closing.is_some_and(|since| now >= since + RESUME_WAIT)
            || (self.held && now >= self.proposed_at + RESUME_WAIT)
        {
            return Err(Error::Closed);
        }
        let unanswered = self.proposed == Some(Carrier::Lane) && now >= self.proposed_at + STALL;
        let stalled = self.sending == Carrier::Lane
            && !self.held
            && !self.unacknowledged.is_empty()
            && now >= self.progress + STALL;
        if unanswered || stalled {
            self.abandon_lane();
        }
        if self.ack_at.is_some_and(|at| now >= at) {
            self.ack().await?;
        }
        Ok(())
    }

    fn due(&self) -> Option<tokio::time::Instant> {
        let waiting = !self.unacknowledged.is_empty();
        [
            self.closing.map(|since| since + RESUME_WAIT),
            self.held.then(|| self.proposed_at + RESUME_WAIT),
            (waiting && !*self.late.borrow()).then(|| self.progress + LATE),
            (self.proposed == Some(Carrier::Lane)).then(|| self.proposed_at + STALL),
            (self.sending == Carrier::Lane && !self.held && waiting).then(|| self.progress + STALL),
            self.ack_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn tell_late(&self) {
        let late =
            !self.unacknowledged.is_empty() && tokio::time::Instant::now() >= self.progress + LATE;
        self.late
            .send_if_modified(|current| std::mem::replace(current, late) != late);
    }

    fn publish(&self) {
        let lane = match (&self.lane, self.sending, self.held) {
            (Some(lane), Carrier::Lane, false) => Some(lane),
            _ => None,
        };
        let path = if lane.is_some() {
            Path::Direct
        } else {
            Path::Relay
        };
        self.path.send_if_modified(|current| {
            let changed = *current != path;
            *current = path;
            changed
        });
        let media = lane.and_then(Lane::media);
        self.link.send_if_modified(|current| {
            let changed = match (current.as_ref(), media.as_ref()) {
                (Some(current), Some(media)) => !current.same(media),
                (None, None) => false,
                _ => true,
            };
            if changed {
                current.clone_from(&media);
            }
            changed
        });
    }
}

async fn next_on(lane: &mut Option<Lane>) -> Option<Vec<u8>> {
    match lane {
        Some(lane) => lane.reader.recv().await,
        None => std::future::pending().await,
    }
}

fn broken() -> Error {
    invalid("The route of the terminal channel is broken.")
}

fn data(sequence: u64, acknowledged: u64, payload: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(17 + payload.len());
    message.push(DATA);
    message.extend_from_slice(&sequence.to_be_bytes());
    message.extend_from_slice(&acknowledged.to_be_bytes());
    message.extend_from_slice(payload);
    message
}

fn numbered(kind: u8, number: u64) -> Vec<u8> {
    let mut message = Vec::with_capacity(9);
    message.push(kind);
    message.extend_from_slice(&number.to_be_bytes());
    message
}

fn data_parts(rest: &[u8]) -> Option<(u64, u64, &[u8])> {
    let (sequence, rest) = rest.split_first_chunk::<8>()?;
    let (acknowledged, payload) = rest.split_first_chunk::<8>()?;
    Some((
        u64::from_be_bytes(*sequence),
        u64::from_be_bytes(*acknowledged),
        payload,
    ))
}

fn number(rest: &[u8]) -> Result<u64> {
    <[u8; 8]>::try_from(rest)
        .map(u64::from_be_bytes)
        .map_err(|_| broken())
}

/// Direct-path discovery for a route. Peer addresses are exchanged inside the encrypted channel
/// so the relay server never sees them.
pub(super) struct Finder {
    frames: mpsc::Receiver<Frame>,
    side: Side,
    /// The pipe whose relay allocation the finder holds until it ends.
    relayed: Option<(Arc<Peers>, Uuid)>,
    stop: CancellationToken,
}

enum Side {
    Viewer {
        answers: mpsc::Sender<Vec<SocketAddr>>,
    },
    #[cfg(feature = "host")]
    Host {
        peers: Arc<Peers>,
        paths: Paths,
        key: Vec<u8>,
        pipe: Uuid,
        frames: mpsc::Sender<Frame>,
        answering: Option<tokio::task::AbortHandle>,
    },
    Relay {
        #[cfg_attr(
            not(feature = "host"),
            expect(dead_code, reason = "only a host answers")
        )]
        frames: mpsc::Sender<Frame>,
    },
}

impl Finder {
    pub(super) fn viewer(
        peers: Option<Arc<Peers>>,
        paths: Paths,
        host_key: Vec<u8>,
        pipe: Uuid,
        link: Arc<Link>,
    ) -> Self {
        let Some(peers) = peers else {
            return Self::relay();
        };
        let (frames, outgoing) = mpsc::channel(4);
        let stop = CancellationToken::new();
        let (answers, answered) = mpsc::channel(1);
        let relayed = Some((Arc::clone(&peers), pipe));
        tokio::spawn(stop.clone().run_until_cancelled_owned(look(
            peers,
            paths,
            (host_key, pipe, link),
            frames,
            answered,
        )));
        Self {
            frames: outgoing,
            side: Side::Viewer { answers },
            relayed,
            stop,
        }
    }

    pub(super) fn relay() -> Self {
        let (frames, outgoing) = mpsc::channel(4);
        Self {
            frames: outgoing,
            side: Side::Relay { frames },
            relayed: None,
            stop: CancellationToken::new(),
        }
    }

    #[cfg(feature = "host")]
    pub(super) fn host(
        peers: Option<Arc<Peers>>,
        paths: Paths,
        viewer_key: Vec<u8>,
        pipe: Uuid,
    ) -> Self {
        let Some(peers) = peers else {
            return Self::relay();
        };
        let (frames, outgoing) = mpsc::channel(4);
        Self {
            frames: outgoing,
            relayed: Some((Arc::clone(&peers), pipe)),
            side: Side::Host {
                peers,
                paths,
                key: viewer_key,
                pipe,
                frames,
                answering: None,
            },
            stop: CancellationToken::new(),
        }
    }

    pub(super) async fn next(&mut self) -> Option<Frame> {
        self.frames.recv().await
    }

    pub(super) fn heard(&mut self, frame: Frame) {
        match (&mut self.side, frame) {
            (Side::Viewer { answers }, Frame::PathAnswer { addresses }) => {
                drop(answers.try_send(addresses));
            }
            #[cfg(feature = "host")]
            (
                Side::Host {
                    peers,
                    paths,
                    key,
                    pipe,
                    frames,
                    answering,
                },
                Frame::PathOffer { addresses },
            ) => {
                if let Some(earlier) = answering.take() {
                    earlier.abort();
                }
                *answering = Some(
                    tokio::spawn(self.stop.clone().run_until_cancelled_owned(answer(
                        Arc::clone(peers),
                        paths.clone(),
                        key.clone(),
                        *pipe,
                        addresses,
                        frames.clone(),
                    )))
                    .abort_handle(),
                );
            }
            #[cfg(feature = "host")]
            (Side::Relay { frames }, Frame::PathOffer { .. }) => {
                drop(frames.try_send(Frame::PathAnswer {
                    addresses: Vec::new(),
                }));
            }
            _ => {}
        }
    }
}

impl Drop for Finder {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some((peers, pipe)) = self.relayed.take() {
            peers.forget_relayed(pipe);
        }
        #[cfg(feature = "host")]
        if let Side::Host {
            answering: Some(answering),
            ..
        } = &self.side
        {
            answering.abort();
        }
    }
}

/// Offers this device's addresses to the host and connects to its answer, straight or through
/// the relay, again after each loss of the connection.
async fn look(
    peers: Arc<Peers>,
    mut paths: Paths,
    (host_key, pipe, link): (Vec<u8>, Uuid, Arc<Link>),
    frames: mpsc::Sender<Frame>,
    mut answers: mpsc::Receiver<Vec<SocketAddr>>,
) {
    let mut failures = 0_u32;
    let mut changes = peers.changes();
    loop {
        let addresses = peers.addresses().await;
        // Drain any late PathAnswer left over from a previous timed-out offer.
        while answers.try_recv().is_ok() {}
        if addresses.is_empty() {
            if changes.changed().await.is_err() {
                return;
            }
            failures = 0;
            continue;
        }
        if peers.relays() {
            link.relay(pipe);
        }
        if frames.send(Frame::PathOffer { addresses }).await.is_err() {
            return;
        }
        let theirs = match tokio::time::timeout(ANSWER_WAIT, answers.recv()).await {
            Ok(Some(theirs)) if !theirs.is_empty() => theirs,
            Ok(_) => return,
            Err(_) => Vec::new(),
        };
        let reached = match peers.reach(&host_key, &theirs, pipe).await {
            Some(lane) => paths.attach(lane) && paths.went_direct().await,
            None => false,
        };
        if reached {
            tracing::info!("a terminal connection went straight to its computer");
            let straight = tokio::time::Instant::now();
            if !paths.back_on_relay().await {
                return;
            }
            tracing::info!("a terminal connection went back to the relay");
            failures = if straight.elapsed() >= STABLE_CONNECTION {
                0
            } else {
                failures.saturating_add(1)
            };
        } else {
            failures = failures.saturating_add(1);
        }
        let pause = match failures {
            0 => Duration::from_secs(5),
            1 => Duration::from_secs(30),
            2 => Duration::from_mins(2),
            _ => Duration::from_mins(10),
        };
        tokio::select! {
            () = tokio::time::sleep(pause) => {}
            Ok(()) = changes.changed() => failures = 0,
        }
    }
}

#[cfg(feature = "host")]
async fn answer(
    peers: Arc<Peers>,
    paths: Paths,
    viewer_key: Vec<u8>,
    pipe: Uuid,
    theirs: Vec<SocketAddr>,
    frames: mpsc::Sender<Frame>,
) {
    let ours = peers.addresses().await;
    let expected = peers.expect(&viewer_key, pipe);
    if frames
        .send(Frame::PathAnswer { addresses: ours })
        .await
        .is_err()
    {
        return;
    }
    if let Some(lane) = expected.meet(&theirs).await {
        paths.attach(lane);
    }
}
