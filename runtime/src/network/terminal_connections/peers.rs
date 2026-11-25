use std::{
    collections::HashMap,
    fmt, io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    sync::{
        Mutex as Locked, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicUsize},
    },
    task::{Context, Poll, ready},
};

use futures_util::stream::FuturesUnordered;
#[cfg(feature = "host")]
use quinn::crypto::rustls::QuicServerConfig;
use quinn::{
    AsyncUdpSocket, TokioRuntime, UdpPoller, VarInt,
    crypto::rustls::QuicClientConfig,
    udp::{RecvMeta, Transmit, UdpSocketState},
};
use rustls::{crypto::aws_lc_rs::cipher_suite, pki_types::CertificateDer};
use tokio::{
    io::Interest,
    sync::{Mutex, watch},
};

use super::{
    channel::{ChannelKeys, device_key},
    *,
};

#[cfg(test)]
mod tests;

const STUN_SERVER: &str = "stun.cloudflare.com:3478";
const STUN_WAIT: Duration = Duration::from_millis(800);
const STUN_KEPT: Duration = Duration::from_secs(30);
const STUN_COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];
const DIAL: Duration = Duration::from_secs(3);
const DIAL_SPACING: Duration = Duration::from_millis(40);
#[cfg(feature = "host")]
const PUNCH_SPACING: Duration = Duration::from_millis(100);
const ARRIVAL: Duration = Duration::from_secs(5);
const KEEP_ALIVE: Duration = Duration::from_secs(10);
const IDLE: Duration = Duration::from_secs(25);
const UNUSED: Duration = Duration::from_mins(1);
const SWEEP: Duration = Duration::from_secs(15);
const MOST_CONNECTIONS: usize = 64;
const MOST_STREAMS: u32 = 256;
const LANE_MESSAGE: usize = 1024 * 1024;
const SERVER_NAME: &str = "kodosi.invalid";
const PORTS: std::ops::RangeInclusive<u16> = 47360..=47379;
const TAKEN: u8 = 1;
const MOST_RESETS: u32 = 64;

/// Manages peer-to-peer QUIC connections between the user's own devices using one UDP socket and
/// endpoint per runtime, bound only while a terminal uses or looks for a direct path.
pub(crate) struct Peers {
    keys: Arc<ChannelKeys>,
    networks: fn() -> Vec<Network>,
    stun: Option<&'static str>,
    bound: Mutex<Option<Arc<Bound>>>,
    dialed: Locked<HashMap<Vec<u8>, Arc<Joined>>>,
    dialing: Locked<HashMap<Vec<u8>, Arc<Mutex<()>>>>,
    #[cfg(feature = "host")]
    arrived: Locked<HashMap<Vec<u8>, Arc<Joined>>>,
    #[cfg(feature = "host")]
    expected: Locked<HashMap<(Vec<u8>, Uuid), oneshot::Sender<Lane>>>,
    reflexive: Locked<Option<(tokio::time::Instant, Vec<SocketAddr>)>>,
    changes: watch::Sender<u64>,
}

struct Bound {
    endpoint: quinn::Endpoint,
    socket: Arc<Demux>,
    client: quinn::ClientConfig,
}

impl Drop for Bound {
    fn drop(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), b"");
    }
}

struct Joined {
    connection: quinn::Connection,
    uses: AtomicUsize,
    unused_since: Locked<tokio::time::Instant>,
}

impl Joined {
    fn new(connection: quinn::Connection) -> Arc<Self> {
        Arc::new(Self {
            connection,
            uses: AtomicUsize::new(0),
            unused_since: Locked::new(tokio::time::Instant::now()),
        })
    }

    fn open(&self) -> bool {
        self.connection.close_reason().is_none()
    }

    fn unused_for(&self, now: tokio::time::Instant) -> Duration {
        if self.uses.load(Ordering::Acquire) > 0 {
            return Duration::ZERO;
        }
        self.unused_since.lock().map_or(Duration::ZERO, |since| {
            now.saturating_duration_since(*since)
        })
    }
}

struct Use(Arc<Joined>);

impl Use {
    fn new(joined: &Arc<Joined>) -> Self {
        joined.uses.fetch_add(1, Ordering::AcqRel);
        Self(Arc::clone(joined))
    }
}

impl Drop for Use {
    fn drop(&mut self) {
        if self.0.uses.fetch_sub(1, Ordering::AcqRel) == 1
            && let Ok(mut since) = self.0.unused_since.lock()
        {
            *since = tokio::time::Instant::now();
        }
    }
}

pub(super) struct Lane {
    writer: Writer,
    pub(super) reader: mpsc::UnboundedReceiver<Vec<u8>>,
    used: Option<Use>,
}

enum Writer {
    Quic(quinn::SendStream),
    #[cfg(test)]
    Memory(mpsc::UnboundedSender<Vec<u8>>, CancellationToken),
}

impl Lane {
    fn quic(send: quinn::SendStream, receive: quinn::RecvStream, used: Use) -> Self {
        let (messages, reader) = mpsc::unbounded_channel();
        tokio::spawn(read_lane(receive, messages));
        Self {
            writer: Writer::Quic(send),
            reader,
            used: Some(used),
        }
    }

    pub(super) async fn write(&mut self, message: &[u8]) -> Result<()> {
        match &mut self.writer {
            Writer::Quic(send) => {
                let length = u32::try_from(message.len()).map_err(|_| Error::Closed)?;
                let mut framed = Vec::with_capacity(4 + message.len());
                framed.extend_from_slice(&length.to_be_bytes());
                framed.extend_from_slice(message);
                send.write_all(&framed).await.map_err(|_| Error::Closed)
            }
            #[cfg(test)]
            Writer::Memory(sender, _) => sender.send(message.to_vec()).map_err(|_| Error::Closed),
        }
    }

    /// Closes the underlying QUIC connection so subsequent retries do not open new streams on a
    /// blackholed path before QUIC's idle timeout expires.
    pub(super) fn abandon(self) {
        match &self.writer {
            Writer::Quic(_) => {
                if let Some(used) = &self.used {
                    used.0.connection.close(VarInt::from_u32(0), b"");
                }
            }
            #[cfg(test)]
            Writer::Memory(_, wire) => wire.cancel(),
        }
    }

    #[cfg(test)]
    pub(super) fn memory(
        writer: mpsc::UnboundedSender<Vec<u8>>,
        reader: mpsc::UnboundedReceiver<Vec<u8>>,
        wire: CancellationToken,
    ) -> Self {
        Self {
            writer: Writer::Memory(writer, wire),
            reader,
            used: None,
        }
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        match &mut self.writer {
            Writer::Quic(send) => drop(send.finish()),
            #[cfg(test)]
            Writer::Memory(..) => {}
        }
    }
}

async fn open_lane(joined: &Arc<Joined>, pipe: Uuid) -> Option<Lane> {
    let used = Use::new(joined);
    let opened = async {
        let (mut send, mut receive) = joined.connection.open_bi().await.ok()?;
        send.write_all(pipe.as_bytes()).await.ok()?;
        let mut taken = [0];
        receive.read_exact(&mut taken).await.ok()?;
        (taken == [TAKEN]).then_some((send, receive))
    };
    let (send, receive) = tokio::time::timeout(ARRIVAL, opened).await.ok()??;
    Some(Lane::quic(send, receive, used))
}

async fn read_lane(mut receive: quinn::RecvStream, messages: mpsc::UnboundedSender<Vec<u8>>) {
    loop {
        let message = tokio::select! {
            () = messages.closed() => {
                drop(receive.stop(VarInt::from_u32(0)));
                return;
            }
            message = read_message(&mut receive) => message,
        };
        let Some(message) = message else {
            return;
        };
        if messages.send(message).is_err() {
            return;
        }
    }
}

async fn read_message(receive: &mut quinn::RecvStream) -> Option<Vec<u8>> {
    let mut length = [0; 4];
    receive.read_exact(&mut length).await.ok()?;
    let length = usize::try_from(u32::from_be_bytes(length)).ok()?;
    if length > LANE_MESSAGE {
        return None;
    }
    let mut message = vec![0; length];
    receive.read_exact(&mut message).await.ok()?;
    Some(message)
}

impl Peers {
    pub(crate) fn new(keys: Arc<ChannelKeys>) -> Arc<Self> {
        Self::with(keys, own_networks, Some(STUN_SERVER))
    }

    #[cfg(all(test, feature = "host"))]
    pub(super) fn on_loopback(keys: Arc<ChannelKeys>) -> Arc<Self> {
        let loopback = || {
            vec![Network {
                ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                mask: IpAddr::V4(Ipv4Addr::new(255, 0, 0, 0)),
            }]
        };
        Self::with(keys, loopback, None)
    }

    fn with(
        keys: Arc<ChannelKeys>,
        networks: fn() -> Vec<Network>,
        stun: Option<&'static str>,
    ) -> Arc<Self> {
        Arc::new(Self {
            keys,
            networks,
            stun,
            bound: Mutex::new(None),
            dialed: Locked::new(HashMap::new()),
            dialing: Locked::new(HashMap::new()),
            #[cfg(feature = "host")]
            arrived: Locked::new(HashMap::new()),
            #[cfg(feature = "host")]
            expected: Locked::new(HashMap::new()),
            reflexive: Locked::new(None),
            changes: watch::Sender::new(0),
        })
    }

    pub(crate) fn uses(&self, keys: &Arc<ChannelKeys>) -> bool {
        Arc::ptr_eq(&self.keys, keys)
    }

    pub(super) async fn addresses(self: &Arc<Self>) -> Vec<SocketAddr> {
        let Ok(bound) = self.bind().await else {
            return Vec::new();
        };
        let Ok(local) = bound.socket.local_addr() else {
            return Vec::new();
        };
        let dual = local.is_ipv6();
        let mut addresses = self.reflexive(&bound, dual).await;
        let mut networks = (self.networks)()
            .into_iter()
            .filter(|network| dual || network.ip.is_ipv4())
            .collect::<Vec<_>>();
        networks.sort_by_key(|network| match network.ip {
            IpAddr::V6(_) => 0,
            IpAddr::V4(ip) if !ip.is_private() => 1,
            IpAddr::V4(ip) if ip.octets()[0] == 172 => 3,
            IpAddr::V4(_) => 2,
        });
        addresses.extend(
            networks
                .into_iter()
                .map(|network| SocketAddr::new(network.ip, local.port())),
        );
        let mut seen = Vec::new();
        addresses.retain(|address| {
            let new = !seen.contains(address);
            seen.push(*address);
            new
        });
        addresses.truncate(wire::MOST_DIRECT_ADDRESSES);
        addresses
    }

    pub(super) async fn reach(
        self: &Arc<Self>,
        host_key: &[u8],
        theirs: &[SocketAddr],
        pipe: Uuid,
    ) -> Option<Lane> {
        let dual = self.bind().await.ok()?.socket.local_addr().ok()?.is_ipv6();
        let candidates = usable(theirs, &(self.networks)(), dual);
        self.reach_at(host_key, candidates, pipe).await
    }

    async fn reach_at(
        self: &Arc<Self>,
        host_key: &[u8],
        candidates: Vec<(SocketAddr, bool)>,
        pipe: Uuid,
    ) -> Option<Lane> {
        let turn = {
            let mut dialing = self.dialing.lock().ok()?;
            Arc::clone(dialing.entry(host_key.to_vec()).or_default())
        };
        let _turn = turn.lock().await;
        let kept = self
            .dialed
            .lock()
            .ok()?
            .get(host_key)
            .filter(|joined| joined.open())
            .map(Arc::clone);
        if let Some(joined) = kept {
            if let Some(lane) = open_lane(&joined, pipe).await {
                return Some(lane);
            }
            // Close the dead connection immediately rather than waiting for QUIC's idle timeout.
            joined.connection.close(VarInt::from_u32(0), b"");
        }
        let joined = self.dial(host_key, candidates).await?;
        open_lane(&joined, pipe).await
    }

    async fn dial(
        self: &Arc<Self>,
        host_key: &[u8],
        candidates: Vec<(SocketAddr, bool)>,
    ) -> Option<Arc<Joined>> {
        let bound = self.bind().await.ok()?;
        if self.dialed.lock().ok()?.len() >= MOST_CONNECTIONS {
            return None;
        }
        let attempts = candidates
            .into_iter()
            .zip(0_u32..)
            .map(|((address, lan), index)| {
                let bound = Arc::clone(&bound);
                async move {
                    tokio::time::sleep(DIAL_SPACING * index).await;
                    let connection = bound
                        .endpoint
                        .connect_with(bound.client.clone(), address, SERVER_NAME)
                        .ok()?
                        .await
                        .ok()?;
                    (identity(&connection).ok()?.as_slice() == host_key)
                        .then_some((connection, lan))
                }
            })
            .collect::<FuturesUnordered<_>>();
        let (connection, lan) = tokio::time::timeout(DIAL, first(attempts))
            .await
            .ok()
            .flatten()?;
        tracing::info!(
            lan,
            "this device reached another device of its person straight"
        );
        let joined = Joined::new(connection);
        self.dialed
            .lock()
            .ok()?
            .insert(host_key.to_vec(), Arc::clone(&joined));
        Some(joined)
    }

    /// Registers an expected direct stream from `viewer_key`; `Expected::meet` sends UDP
    /// hole-punch packets to the viewer's addresses until a QUIC connection is established.
    #[cfg(feature = "host")]
    pub(super) fn expect(self: &Arc<Self>, viewer_key: &[u8], pipe: Uuid) -> Expected {
        let (sender, lane) = oneshot::channel();
        let key = (viewer_key.to_vec(), pipe);
        if let Ok(mut expected) = self.expected.lock() {
            expected.insert(key.clone(), sender);
        }
        Expected {
            peers: Arc::clone(self),
            key,
            lane,
        }
    }

    async fn bind(self: &Arc<Self>) -> Result<Arc<Bound>> {
        let mut bound = self.bound.lock().await;
        if let Some(bound) = bound
            .as_ref()
            .filter(|bound| !bound.socket.broken.load(Ordering::Acquire))
        {
            return Ok(Arc::clone(bound));
        }
        let socket = udp_socket().map_err(|_| Error::Closed)?;
        let socket = Arc::new(Demux {
            dual: socket.local_addr().map_err(|_| Error::Closed)?.is_ipv6(),
            inner: Arc::new(Socket::new(socket).map_err(|_| Error::Closed)?),
            asked: Locked::new(HashMap::new()),
            broken: AtomicBool::new(false),
            resets: AtomicU32::new(0),
        });
        let mut config = quinn::EndpointConfig::default();
        config.grease_quic_bit(false);
        #[cfg(feature = "host")]
        let server = Some(self.server()?);
        #[cfg(not(feature = "host"))]
        let server = None;
        let endpoint = quinn::Endpoint::new_with_abstract_socket(
            config,
            server,
            Arc::clone(&socket) as Arc<dyn AsyncUdpSocket>,
            Arc::new(TokioRuntime),
        )
        .map_err(|_| Error::Closed)?;
        let client = self.client()?;
        #[cfg(feature = "host")]
        tokio::spawn(accept(Arc::downgrade(self), endpoint.clone()));
        self.forget_reflexive();
        let fresh = Arc::new(Bound {
            endpoint,
            socket,
            client,
        });
        tokio::spawn(sweep(Arc::downgrade(self), Arc::downgrade(&fresh)));
        *bound = Some(Arc::clone(&fresh));
        drop(bound);
        Ok(fresh)
    }

    pub(crate) fn network_changed(self: &Arc<Self>) {
        let peers = Arc::clone(self);
        tokio::spawn(async move {
            peers.forget_bound().await;
            peers.changes.send_modify(|changes| *changes += 1);
        });
    }

    pub(super) fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub(crate) async fn forget_bound(&self) {
        *self.bound.lock().await = None;
        if let Ok(mut dialed) = self.dialed.lock() {
            dialed.clear();
        }
        #[cfg(feature = "host")]
        if let Ok(mut arrived) = self.arrived.lock() {
            arrived.clear();
        }
        self.forget_reflexive();
    }

    fn forget_reflexive(&self) {
        if let Ok(mut kept) = self.reflexive.lock() {
            *kept = None;
        }
    }

    fn client(&self) -> Result<quinn::ClientConfig> {
        let crypto = QuicClientConfig::with_initial(self.keys.direct_client(), initial_suite()?)
            .map_err(|_| invalid("The direct connection settings are not valid."))?;
        let mut client = quinn::ClientConfig::new(Arc::new(crypto));
        client.transport_config(transport());
        Ok(client)
    }

    #[cfg(feature = "host")]
    fn server(&self) -> Result<quinn::ServerConfig> {
        let crypto = QuicServerConfig::with_initial(self.keys.direct_server(), initial_suite()?)
            .map_err(|_| invalid("The direct connection settings are not valid."))?;
        let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
        server.transport_config(transport());
        Ok(server)
    }

    async fn reflexive(&self, bound: &Bound, dual: bool) -> Vec<SocketAddr> {
        if let Ok(kept) = self.reflexive.lock()
            && let Some((at, addresses)) = kept.as_ref()
            && at.elapsed() < STUN_KEPT
        {
            return addresses.clone();
        }
        let Some(server) = self.stun else {
            return Vec::new();
        };
        let servers = tokio::time::timeout(STUN_WAIT, tokio::net::lookup_host(server))
            .await
            .ok()
            .and_then(std::result::Result::ok)
            .map_or_else(Vec::new, Iterator::collect::<Vec<_>>);
        let ipv4 = servers.iter().find(|server| server.is_ipv4()).copied();
        let ipv6 = servers
            .iter()
            .find(|server| server.is_ipv6())
            .copied()
            .filter(|_| dual);
        let (seen4, seen6) = tokio::join!(
            async {
                match ipv4 {
                    Some(server) => bound.socket.stun(server).await,
                    None => None,
                }
            },
            async {
                match ipv6 {
                    Some(server) => bound.socket.stun(server).await,
                    None => None,
                }
            },
        );
        let addresses = seen4.into_iter().chain(seen6).collect::<Vec<_>>();
        if let Ok(mut kept) = self.reflexive.lock() {
            *kept = Some((tokio::time::Instant::now(), addresses.clone()));
        }
        addresses
    }

    fn trim(&self) -> bool {
        let now = tokio::time::Instant::now();
        let keep = |_: &Vec<u8>, joined: &mut Arc<Joined>| {
            if joined.open() && joined.unused_for(now) >= UNUSED {
                joined.connection.close(VarInt::from_u32(0), b"");
            }
            joined.open()
        };
        #[cfg_attr(
            not(feature = "host"),
            expect(unused_mut, reason = "only a host takes connections")
        )]
        let mut busy = self.dialed.lock().is_ok_and(|mut dialed| {
            dialed.retain(keep);
            !dialed.is_empty()
        });
        #[cfg(feature = "host")]
        {
            busy |= self.arrived.lock().is_ok_and(|mut arrived| {
                arrived.retain(keep);
                !arrived.is_empty()
            });
            busy |= self
                .expected
                .lock()
                .is_ok_and(|expected| !expected.is_empty());
        }
        if let Ok(mut dialing) = self.dialing.lock() {
            dialing.retain(|_, turn| Arc::strong_count(turn) > 1);
        }
        busy
    }
}

async fn sweep(peers: Weak<Peers>, own: Weak<Bound>) {
    let mut idle_since = None;
    loop {
        tokio::time::sleep(SWEEP).await;
        let Some(peers) = peers.upgrade() else {
            return;
        };
        if !holds(peers.bound.lock().await.as_ref(), &own) {
            return;
        }
        if peers.trim() {
            idle_since = None;
            continue;
        }
        let since = *idle_since.get_or_insert_with(tokio::time::Instant::now);
        if since.elapsed() >= UNUSED {
            let mut bound = peers.bound.lock().await;
            if !holds(bound.as_ref(), &own) {
                return;
            }
            if !peers.trim() {
                *bound = None;
                return;
            }
            drop(bound);
            idle_since = None;
        }
    }
}

fn holds(bound: Option<&Arc<Bound>>, own: &Weak<Bound>) -> bool {
    bound.is_some_and(|bound| std::ptr::eq(Arc::as_ptr(bound), own.as_ptr()))
}

#[cfg(feature = "host")]
pub(super) struct Expected {
    peers: Arc<Peers>,
    key: (Vec<u8>, Uuid),
    lane: oneshot::Receiver<Lane>,
}

#[cfg(feature = "host")]
impl Expected {
    pub(super) async fn meet(mut self, theirs: &[SocketAddr]) -> Option<Lane> {
        let bound = self.peers.bind().await.ok()?;
        let dual = bound.socket.local_addr().ok()?.is_ipv6();
        let joined = self
            .peers
            .arrived
            .lock()
            .ok()?
            .get(&self.key.0)
            .is_some_and(|joined| joined.open());
        let targets = usable(theirs, &(self.peers.networks)(), dual);
        let punch = async {
            if !joined {
                let until = tokio::time::Instant::now() + DIAL;
                while tokio::time::Instant::now() < until {
                    for (address, _) in &targets {
                        bound.socket.send_to(*address, &[0]);
                    }
                    tokio::time::sleep(PUNCH_SPACING).await;
                }
            }
            std::future::pending::<()>().await;
        };
        tokio::select! {
            lane = tokio::time::timeout(ARRIVAL, &mut self.lane) => lane.ok()?.ok(),
            () = punch => None,
        }
    }
}

#[cfg(feature = "host")]
impl Drop for Expected {
    fn drop(&mut self) {
        if let Ok(mut expected) = self.peers.expected.lock() {
            expected.remove(&self.key);
        }
    }
}

#[cfg(feature = "host")]
async fn accept(peers: Weak<Peers>, endpoint: quinn::Endpoint) {
    while let Some(incoming) = endpoint.accept().await {
        let expecting = peers.upgrade().is_some_and(|peers| {
            peers
                .arrived
                .lock()
                .is_ok_and(|arrived| arrived.len() < MOST_CONNECTIONS)
                && peers
                    .expected
                    .lock()
                    .is_ok_and(|expected| !expected.is_empty())
        });
        if !expecting {
            incoming.refuse();
            continue;
        }
        tokio::spawn(welcome(Weak::clone(&peers), incoming));
    }
}

/// Accepts a connection only from a device that one of this host's routes expects, and hands each
/// of its streams to the route waiting for it.
#[cfg(feature = "host")]
async fn welcome(peers: Weak<Peers>, incoming: quinn::Incoming) {
    let Ok(Ok(connection)) = tokio::time::timeout(ARRIVAL, incoming).await else {
        return;
    };
    let Ok(key) = identity(&connection) else {
        connection.close(VarInt::from_u32(1), b"");
        return;
    };
    let joined = {
        let Some(peers) = peers.upgrade() else {
            return;
        };
        let expected = peers
            .expected
            .lock()
            .is_ok_and(|expected| expected.keys().any(|(device, _)| *device == key));
        if !expected {
            connection.close(VarInt::from_u32(1), b"");
            return;
        }
        let joined = Joined::new(connection.clone());
        if let Ok(mut arrived) = peers.arrived.lock() {
            arrived.insert(key.clone(), Arc::clone(&joined));
        }
        joined
    };
    while let Ok((send, mut receive)) = connection.accept_bi().await {
        let mut pipe = [0; 16];
        if !matches!(
            tokio::time::timeout(ARRIVAL, receive.read_exact(&mut pipe)).await,
            Ok(Ok(()))
        ) {
            continue;
        }
        let Some(peers) = peers.upgrade() else {
            return;
        };
        let waiting = peers
            .expected
            .lock()
            .ok()
            .and_then(|mut expected| expected.remove(&(key.clone(), Uuid::from_bytes(pipe))));
        let Some(waiting) = waiting else {
            continue;
        };
        let mut send = send;
        if matches!(
            tokio::time::timeout(ARRIVAL, send.write_all(&[TAKEN])).await,
            Ok(Ok(()))
        ) {
            let _taken = waiting.send(Lane::quic(send, receive, Use::new(&joined)));
        }
    }
}

async fn first<T>(mut attempts: FuturesUnordered<impl Future<Output = Option<T>>>) -> Option<T> {
    while let Some(attempt) = attempts.next().await {
        if attempt.is_some() {
            return attempt;
        }
    }
    None
}

fn identity(connection: &quinn::Connection) -> Result<Vec<u8>> {
    let certificates = connection
        .peer_identity()
        .and_then(|identity| identity.downcast::<Vec<CertificateDer<'static>>>().ok());
    device_key(
        certificates
            .as_deref()
            .and_then(|certificates| certificates.first()),
    )
}

fn initial_suite() -> Result<rustls::quic::Suite> {
    cipher_suite::TLS13_AES_128_GCM_SHA256
        .tls13()
        .and_then(rustls::Tls13CipherSuite::quic_suite)
        .ok_or_else(|| invalid("The direct connection settings are not valid."))
}

fn transport() -> Arc<quinn::TransportConfig> {
    let mut transport = quinn::TransportConfig::default();
    transport.keep_alive_interval(Some(KEEP_ALIVE));
    transport.max_idle_timeout(quinn::IdleTimeout::try_from(IDLE).ok());
    transport.max_concurrent_bidi_streams(VarInt::from_u32(MOST_STREAMS));
    transport.max_concurrent_uni_streams(VarInt::from_u32(0));
    Arc::new(transport)
}

/// Prefers ports in `PORTS` (opened on private networks by the Windows package), falling back
/// to an ephemeral port if all are in use.
fn udp_socket() -> io::Result<std::net::UdpSocket> {
    let ports = || PORTS.chain([0]);
    let socket = ports()
        .find_map(|port| dual_stack(port).ok())
        .map(std::net::UdpSocket::from)
        .or_else(|| {
            ports().find_map(|port| std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).ok())
        })
        .ok_or_else(|| io::Error::from(io::ErrorKind::AddrNotAvailable))?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

fn dual_stack(port: u16) -> io::Result<socket2::Socket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_only_v6(false)?;
    socket.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)).into())?;
    Ok(socket)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Network {
    ip: IpAddr,
    mask: IpAddr,
}

fn own_networks() -> Vec<Network> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(if_addrs::Interface::is_oper_up)
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(address) if !address.ip.is_link_local() => Some(Network {
                ip: IpAddr::V4(address.ip),
                mask: IpAddr::V4(address.netmask),
            }),
            if_addrs::IfAddr::V6(address)
                if !address.ip.is_unicast_link_local() && !address.ip.is_unspecified() =>
            {
                Some(Network {
                    ip: IpAddr::V6(address.ip),
                    mask: IpAddr::V6(address.netmask),
                })
            }
            _ => None,
        })
        .collect()
}

fn same_network(network: &Network, ip: IpAddr) -> bool {
    match (network.ip, network.mask, ip) {
        (IpAddr::V4(own), IpAddr::V4(mask), IpAddr::V4(other)) => {
            own.to_bits() & mask.to_bits() == other.to_bits() & mask.to_bits()
        }
        (IpAddr::V6(own), IpAddr::V6(mask), IpAddr::V6(other)) => {
            own.to_bits() & mask.to_bits() == other.to_bits() & mask.to_bits()
        }
        _ => false,
    }
}

fn is_lan(ip: IpAddr, networks: &[Network]) -> bool {
    let ip = ip.to_canonical();
    networks.iter().any(|network| same_network(network, ip))
}

/// Filters and orders dialable peer addresses with an `is_lan` flag. Skips private/loopback
/// addresses outside our local subnets so we do not trigger pointless OS local-network prompts.
fn usable(theirs: &[SocketAddr], networks: &[Network], dual: bool) -> Vec<(SocketAddr, bool)> {
    let mut usable = theirs
        .iter()
        .map(|address| SocketAddr::new(address.ip().to_canonical(), address.port()))
        .filter(|address| address.port() != 0 && (dual || address.is_ipv4()))
        .filter_map(|address| {
            let lan = is_lan(address.ip(), networks);
            let worth = match address.ip() {
                IpAddr::V4(ip) => {
                    !(ip.is_unspecified()
                        || ip.is_link_local()
                        || ip.is_multicast()
                        || ip.is_broadcast())
                        && (lan || !(ip.is_private() || ip.is_loopback()))
                }
                IpAddr::V6(ip) => {
                    !(ip.is_unspecified() || ip.is_unicast_link_local() || ip.is_multicast())
                        && (lan || !(ip.is_unique_local() || ip.is_loopback()))
                }
            };
            worth.then_some((address, lan))
        })
        .collect::<Vec<_>>();
    usable.sort_by_key(|(address, lan)| (!lan, address.is_ipv4()));
    usable.dedup();
    usable
}

#[derive(Debug)]
struct Demux {
    inner: Arc<Socket>,
    dual: bool,
    asked: Locked<HashMap<[u8; 12], oneshot::Sender<SocketAddr>>>,
    broken: AtomicBool,
    resets: AtomicU32,
}

impl Demux {
    fn send_to(&self, destination: SocketAddr, contents: &[u8]) {
        let destination = match destination {
            SocketAddr::V4(address) if self.dual => {
                SocketAddr::new(IpAddr::V6(address.ip().to_ipv6_mapped()), address.port())
            }
            destination => destination,
        };
        drop(self.inner.try_send(&Transmit {
            destination,
            ecn: None,
            contents,
            segment_size: None,
            src_ip: None,
        }));
    }

    async fn stun(&self, server: SocketAddr) -> Option<SocketAddr> {
        for _ in 0..2 {
            let mut id = [0; 12];
            aws_lc_rs::rand::fill(&mut id).ok()?;
            let (sender, answer) = oneshot::channel();
            self.asked.lock().ok()?.insert(id, sender);
            self.send_to(server, &stun_request(id));
            let answered = tokio::time::timeout(STUN_WAIT, answer).await;
            self.asked.lock().ok()?.remove(&id);
            if let Ok(Ok(address)) = answered {
                return Some(address);
            }
        }
        None
    }

    fn absorbed(&self, datagram: &[u8]) -> bool {
        if datagram.first().is_none_or(|first| first & 0x40 != 0) {
            return false;
        }
        if let Some((id, address)) = stun_answer(datagram)
            && let Some(asked) = self
                .asked
                .lock()
                .ok()
                .and_then(|mut asked| asked.remove(&id))
        {
            let _sent = asked.send(address);
        }
        true
    }
}

impl AsyncUdpSocket for Demux {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Arc::clone(&self.inner).create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        self.inner.try_send(transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let (Some(buffer), Some(details)) = (bufs.first_mut(), meta.first_mut()) else {
            return Poll::Ready(Ok(0));
        };
        loop {
            match self.inner.poll_recv(
                cx,
                std::slice::from_mut(buffer),
                std::slice::from_mut(details),
            ) {
                Poll::Ready(Ok(0)) => return Poll::Ready(Ok(0)),
                Poll::Ready(Ok(_)) => {
                    self.resets.store(0, Ordering::Relaxed);
                    if details.len == details.stride && self.absorbed(&buffer[..details.len]) {
                        continue;
                    }
                    return Poll::Ready(Ok(1));
                }
                // quinn retries immediately on ConnectionReset and closes the endpoint on other
                // errors, so cap consecutive resets and convert to NotConnected to avoid spinning.
                Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::ConnectionReset => {
                    if self.resets.fetch_add(1, Ordering::Relaxed) < MOST_RESETS {
                        return Poll::Ready(Err(error));
                    }
                    self.broken.store(true, Ordering::Release);
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::NotConnected, error)));
                }
                Poll::Ready(Err(error)) => {
                    self.broken.store(true, Ordering::Release);
                    return Poll::Ready(Err(error));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

/// Workaround for quinn 0.11.12's Tokio socket spinning on persistent read errors (e.g., sockets
/// invalidated when iOS backgrounds an app). Propagates read errors directly to quinn, matching
/// quinn-rs/quinn#2721.
#[derive(Debug)]
struct Socket {
    io: tokio::net::UdpSocket,
    state: UdpSocketState,
}

impl Socket {
    fn new(socket: std::net::UdpSocket) -> io::Result<Self> {
        Ok(Self {
            state: UdpSocketState::new((&socket).into())?,
            io: tokio::net::UdpSocket::from_std(socket)?,
        })
    }
}

impl AsyncUdpSocket for Socket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(Writable {
            socket: self,
            waiting: None,
        })
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        self.io.try_io(Interest::WRITABLE, || {
            self.state.send((&self.io).into(), transmit)
        })
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            ready!(self.io.poll_recv_ready(cx))?;
            match self.io.try_io(Interest::READABLE, || {
                self.state.recv((&self.io).into(), bufs, meta)
            }) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                read => return Poll::Ready(read),
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.state.max_gso_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.state.gro_segments()
    }

    fn may_fragment(&self) -> bool {
        self.state.may_fragment()
    }
}

/// Gives each connection its own `writable()` future because Tokio only wakes the last task
/// that polled a socket's readiness directly.
struct Writable {
    socket: Arc<Socket>,
    waiting: Option<Pin<Box<dyn Future<Output = io::Result<()>> + Send + Sync>>>,
}

impl fmt::Debug for Writable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Writable").finish_non_exhaustive()
    }
}

impl UdpPoller for Writable {
    fn poll_writable(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        let waiting = this.waiting.get_or_insert_with(|| {
            let socket = Arc::clone(&this.socket);
            Box::pin(async move { socket.io.writable().await })
        });
        let writable = waiting.as_mut().poll(cx);
        if writable.is_ready() {
            this.waiting = None;
        }
        writable
    }
}

fn stun_request(id: [u8; 12]) -> [u8; 20] {
    let mut request = [0; 20];
    request[1] = 0x01;
    request[4..8].copy_from_slice(&STUN_COOKIE);
    request[8..].copy_from_slice(&id);
    request
}

/// Parses the XOR-MAPPED-ADDRESS attribute from an RFC 8489 STUN Binding Response.
fn stun_answer(datagram: &[u8]) -> Option<([u8; 12], SocketAddr)> {
    if datagram.get(..2)? != [0x01, 0x01] || datagram.get(4..8)? != STUN_COOKIE {
        return None;
    }
    let id: [u8; 12] = datagram.get(8..20)?.try_into().ok()?;
    let length = usize::from(u16::from_be_bytes(datagram.get(2..4)?.try_into().ok()?));
    let mut attributes = datagram.get(20..20 + length)?;
    while attributes.len() >= 4 {
        let kind = u16::from_be_bytes([attributes[0], attributes[1]]);
        let size = usize::from(u16::from_be_bytes([attributes[2], attributes[3]]));
        let value = attributes.get(4..4 + size)?;
        if kind == 0x0020 && size >= 8 {
            let port = u16::from_be_bytes([value[2], value[3]]) ^ 0x2112;
            let ip = match (value[1], size) {
                (0x01, 8) => {
                    let mut octets: [u8; 4] = value[4..8].try_into().ok()?;
                    for (octet, key) in octets.iter_mut().zip(STUN_COOKIE) {
                        *octet ^= key;
                    }
                    IpAddr::V4(Ipv4Addr::from(octets))
                }
                (0x02, 20) => {
                    let mut octets: [u8; 16] = value[4..20].try_into().ok()?;
                    for (octet, key) in octets.iter_mut().zip(STUN_COOKIE.iter().chain(&id)) {
                        *octet ^= key;
                    }
                    IpAddr::V6(Ipv6Addr::from(octets))
                }
                _ => return None,
            };
            return Some((id, SocketAddr::new(ip, port)));
        }
        attributes = attributes
            .get(4 + size.next_multiple_of(4)..)
            .unwrap_or_default();
    }
    None
}
