use std::{
    collections::HashMap,
    fmt, io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    num::NonZeroUsize,
    pin::Pin,
    sync::{
        Mutex as Locked, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicUsize},
    },
    task::{Context, Poll, ready},
};

use futures_util::stream::FuturesUnordered;
#[cfg(feature = "host")]
use noq::crypto::rustls::QuicServerConfig;
use noq::{
    AsyncUdpSocket, TokioRuntime, UdpSender, VarInt,
    crypto::rustls::QuicClientConfig,
    udp::{RecvMeta, Transmit, UdpSocketState},
};
use rustls::{crypto::aws_lc_rs::cipher_suite, pki_types::CertificateDer};
#[cfg(feature = "host")]
use std::sync::atomic::AtomicU16;
use stream::media;
use tokio::{
    io::Interest,
    sync::{Mutex, watch},
};

use super::{
    channel::{ChannelKeys, device_key},
    scream, *,
};

mod relay;
#[cfg(test)]
mod tests;

pub(super) use relay::Grant;

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
const DATAGRAMS_WAITING: usize = 64 * 1024;
const DATAGRAMS_ARRIVING: usize = 4 * 1024 * 1024;
/// The viewer acknowledges every ninth packet, or after `ACK_DELAY`, which is often enough for
/// `SCReAMv2` and spares slow uplinks.
const ACK_AFTER: u32 = 8;
const ACK_DELAY: Duration = Duration::from_millis(5);
#[cfg(feature = "host")]
const FLOW_REPORTS: usize = 64;
/// A 4K keyframe comes in several hundred shards at once.
const FLOW_SHARDS: usize = 4096;
const LEAST_DATAGRAM: usize = 1162;
const FIRST_RTT: Duration = Duration::from_millis(50);

/// Manages peer-to-peer QUIC connections between the user's own devices using one UDP socket and
/// endpoint per runtime, bound only while a terminal uses or looks for a direct path.
pub(crate) struct Peers {
    keys: Arc<ChannelKeys>,
    networks: fn() -> Vec<Network>,
    /// The Kodosi server's UDP relay, `host:port`, which also answers STUN; the server names it
    /// when the device connects.
    relay: Locked<Option<String>>,
    /// The relay's allocations that the server gave for the devices of each pipe.
    grants: Locked<HashMap<Uuid, Grant>>,
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
    endpoint: noq::Endpoint,
    socket: Demux,
    client: noq::ClientConfig,
}

impl Drop for Bound {
    fn drop(&mut self) {
        self.endpoint.close(VarInt::from_u32(0), b"");
    }
}

struct Joined {
    connection: noq::Connection,
    uses: AtomicUsize,
    unused_since: Locked<tokio::time::Instant>,
    /// Where each video datagram goes, by its kind and flow.
    flows: Locked<HashMap<(u8, u16), mpsc::Sender<Bytes>>>,
    /// The task that reads the datagrams, which holds the connection open while it runs.
    sorting: Locked<Option<tokio::task::AbortHandle>>,
    #[cfg(feature = "host")]
    next_flow: AtomicU16,
    #[cfg(feature = "host")]
    carried: Locked<Option<Arc<scream::Carried>>>,
}

impl Joined {
    fn new(connection: noq::Connection) -> Arc<Self> {
        let joined = Arc::new(Self {
            connection: connection.clone(),
            uses: AtomicUsize::new(0),
            unused_since: Locked::new(tokio::time::Instant::now()),
            flows: Locked::new(HashMap::new()),
            sorting: Locked::new(None),
            #[cfg(feature = "host")]
            next_flow: AtomicU16::new(0),
            #[cfg(feature = "host")]
            carried: Locked::new(None),
        });
        let sorting = tokio::spawn(sort_datagrams(Arc::downgrade(&joined), connection));
        if let Ok(mut kept) = joined.sorting.lock() {
            *kept = Some(sorting.abort_handle());
        }
        joined
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

async fn sort_datagrams(joined: Weak<Joined>, connection: noq::Connection) {
    while let Ok(datagram) = connection.read_datagram().await {
        let Some(joined) = joined.upgrade() else {
            return;
        };
        let Some(route) = media::route(&datagram) else {
            continue;
        };
        let Ok(mut flows) = joined.flows.lock() else {
            return;
        };
        let gone = flows.get(&route).is_some_and(|flow| {
            matches!(
                flow.try_send(datagram),
                Err(mpsc::error::TrySendError::Closed(_))
            )
        });
        if gone {
            flows.remove(&route);
        }
    }
}

/// A device pair's QUIC connection as video uses it: datagrams sorted by kind and flow, the
/// bitrate that its congestion controller finds, and its round trip.
#[derive(Clone)]
pub(super) struct Media(Arc<Joined>);

impl Media {
    /// Starts a flow that this device sends, and returns its number with the viewer's reports.
    #[cfg(feature = "host")]
    pub(super) fn host(&self) -> Option<(u16, mpsc::Receiver<Bytes>)> {
        let (sender, reports) = mpsc::channel(FLOW_REPORTS);
        let flow = {
            let mut flows = self.0.flows.lock().ok()?;
            flows.retain(|_, flow| !flow.is_closed());
            let flow = (0..=u16::MAX)
                .map(|_| self.0.next_flow.fetch_add(1, Ordering::Relaxed))
                .find(|flow| !flows.contains_key(&(media::REPORT, *flow)))?;
            flows.insert((media::REPORT, flow), sender);
            flow
        };
        Some((flow, reports))
    }

    /// Takes the shards of a flow that the other device sends.
    pub(super) fn view(&self, flow: u16) -> Option<mpsc::Receiver<Bytes>> {
        let (sender, shards) = mpsc::channel(FLOW_SHARDS);
        self.0
            .flows
            .lock()
            .ok()?
            .insert((media::SHARD, flow), sender);
        Some(shards)
    }

    /// Flows that this device sends on the connection, which share its bitrate.
    #[cfg(feature = "host")]
    pub(super) fn hosted(&self) -> usize {
        self.0.flows.lock().map_or(1, |flows| {
            flows
                .iter()
                .filter(|((kind, _), flow)| *kind == media::REPORT && !flow.is_closed())
                .count()
        })
    }

    /// Sends a datagram unless that would push out earlier ones that still wait.
    pub(super) fn send(&self, datagram: Bytes) -> bool {
        self.0.connection.datagram_send_buffer_space() >= datagram.len()
            && self.0.connection.send_datagram(datagram).is_ok()
    }

    pub(super) fn datagram_bytes(&self) -> usize {
        self.0
            .connection
            .max_datagram_size()
            .unwrap_or(LEAST_DATAGRAM)
    }

    pub(super) fn rtt(&self) -> Duration {
        self.0
            .connection
            .rtt(noq::PathId::ZERO)
            .unwrap_or(FIRST_RTT)
    }

    /// The bitrate that the path carries, in bits per second.
    #[cfg(feature = "host")]
    pub(super) fn carried(&self) -> Option<u64> {
        let mut kept = self.0.carried.lock().ok()?;
        if kept.is_none() {
            *kept = scream::carried(self.0.connection.congestion_state(noq::PathId::ZERO)?);
        }
        kept.as_ref().map(|carried| carried.get())
    }

    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Drop for Joined {
    fn drop(&mut self) {
        if let Some(sorting) = self.sorting.get_mut().ok().and_then(Option::take) {
            sorting.abort();
        }
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
    Quic(noq::SendStream),
    #[cfg(test)]
    Memory(mpsc::UnboundedSender<Vec<u8>>, CancellationToken),
}

impl Lane {
    fn quic(send: noq::SendStream, receive: noq::RecvStream, used: Use) -> Self {
        let (messages, reader) = mpsc::unbounded_channel();
        tokio::spawn(read_lane(receive, messages));
        Self {
            writer: Writer::Quic(send),
            reader,
            used: Some(used),
        }
    }

    /// The connection under the lane, for video.
    pub(super) fn media(&self) -> Option<Media> {
        self.used.as_ref().map(|used| Media(Arc::clone(&used.0)))
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

async fn read_lane(mut receive: noq::RecvStream, messages: mpsc::UnboundedSender<Vec<u8>>) {
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

async fn read_message(receive: &mut noq::RecvStream) -> Option<Vec<u8>> {
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
        Self::with(keys, own_networks)
    }

    #[cfg(all(test, feature = "host"))]
    pub(super) fn on_loopback(keys: Arc<ChannelKeys>) -> Arc<Self> {
        let loopback = || {
            vec![Network {
                ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                mask: IpAddr::V4(Ipv4Addr::new(255, 0, 0, 0)),
            }]
        };
        Self::with(keys, loopback)
    }

    fn with(keys: Arc<ChannelKeys>, networks: fn() -> Vec<Network>) -> Arc<Self> {
        Arc::new(Self {
            keys,
            networks,
            relay: Locked::new(None),
            grants: Locked::new(HashMap::new()),
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

    /// Takes the relay that the Kodosi server named when the device connected, or none.
    pub(crate) fn relay_at(&self, relay: Option<String>) {
        if let Ok(mut kept) = self.relay.lock() {
            *kept = relay;
        }
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

    /// Connects straight to the host, or through the relay when no straight path comes.
    pub(super) async fn reach(
        self: &Arc<Self>,
        host_key: &[u8],
        theirs: &[SocketAddr],
        pipe: Uuid,
    ) -> Option<Lane> {
        let bound = self.bind().await.ok()?;
        let dual = bound.socket.local_addr().ok()?.is_ipv6();
        let candidates = usable(theirs, &(self.networks)(), dual);
        if let Some(lane) = self.reach_at(host_key, candidates, pipe).await {
            return Some(lane);
        }
        let stand_in = self.through_relay(pipe, &bound).await?;
        self.reach_at(host_key, vec![(stand_in, false)], pipe).await
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
                        .then_some((connection, lan, address))
                }
            })
            .collect::<FuturesUnordered<_>>();
        let (connection, lan, address) = tokio::time::timeout(DIAL, first(attempts))
            .await
            .ok()
            .flatten()?;
        if bound.socket.relays(address) {
            tracing::info!("this device reached another device of its person through the relay");
        } else {
            tracing::info!(
                lan,
                "this device reached another device of its person straight"
            );
        }
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
        if let Some(bound) = bound.as_ref().filter(|bound| !bound.socket.broken()) {
            return Ok(Arc::clone(bound));
        }
        let socket =
            Demux::new(udp_socket().map_err(|_| Error::Closed)?).map_err(|_| Error::Closed)?;
        let mut config = noq::EndpointConfig::default();
        config.grease_quic_bit(false);
        #[cfg(feature = "host")]
        let server = Some(self.server()?);
        #[cfg(not(feature = "host"))]
        let server = None;
        let endpoint = noq::Endpoint::new_with_abstract_socket(
            config,
            server,
            Box::new(socket.clone()),
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

    fn client(&self) -> Result<noq::ClientConfig> {
        let crypto = QuicClientConfig::with_initial(self.keys.direct_client(), initial_suite()?)
            .map_err(|_| invalid("The direct connection settings are not valid."))?;
        let mut client = noq::ClientConfig::new(Arc::new(crypto));
        client.transport_config(transport());
        Ok(client)
    }

    #[cfg(feature = "host")]
    fn server(&self) -> Result<noq::ServerConfig> {
        let crypto = QuicServerConfig::with_initial(self.keys.direct_server(), initial_suite()?)
            .map_err(|_| invalid("The direct connection settings are not valid."))?;
        let mut server = noq::ServerConfig::with_crypto(Arc::new(crypto));
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
        let Some(server) = self.relay.lock().ok().and_then(|relay| relay.clone()) else {
            return Vec::new();
        };
        let servers = tokio::time::timeout(STUN_WAIT, tokio::net::lookup_host(server.as_str()))
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
        let relay = async {
            self.peers.relay_route(self.key.1, &bound).await;
            std::future::pending::<()>().await;
        };
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
            () = relay => None,
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
async fn accept(peers: Weak<Peers>, endpoint: noq::Endpoint) {
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
async fn welcome(peers: Weak<Peers>, incoming: noq::Incoming) {
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

fn identity(connection: &noq::Connection) -> Result<Vec<u8>> {
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

fn transport() -> Arc<noq::TransportConfig> {
    let mut transport = noq::TransportConfig::default();
    transport.keep_alive_interval(Some(KEEP_ALIVE));
    transport.max_idle_timeout(noq::IdleTimeout::try_from(IDLE).ok());
    transport.max_concurrent_bidi_streams(VarInt::from_u32(MOST_STREAMS));
    transport.max_concurrent_uni_streams(VarInt::from_u32(0));
    transport.congestion_controller_factory(Arc::new(scream::Scream));
    // Video waits in its sender, which drops what comes too late, not in noq's queue.
    transport.datagram_send_buffer_size(DATAGRAMS_WAITING);
    transport.datagram_receive_buffer_size(Some(DATAGRAMS_ARRIVING));
    let mut acks = noq::AckFrequencyConfig::default();
    acks.ack_eliciting_threshold(VarInt::from_u32(ACK_AFTER))
        .max_ack_delay(Some(ACK_DELAY));
    transport.ack_frequency_config(Some(acks));
    // A batch of segments leaves at once, and policed mobile links drop the tail of such bursts.
    transport.enable_segmentation_offload(false);
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

/// The runtime's one UDP socket, shared by the QUIC endpoint and by STUN and hole punching. STUN
/// answers never reach QUIC: their first byte lacks the QUIC fixed bit, which Kodosi never greases.
#[derive(Clone, Debug)]
struct Demux(Arc<Shared>);

#[derive(Debug)]
struct Shared {
    io: tokio::net::UdpSocket,
    state: UdpSocketState,
    dual: bool,
    asked: Locked<HashMap<[u8; 12], oneshot::Sender<SocketAddr>>>,
    relays: Locked<relay::Routes>,
    leftover: Locked<Option<relay::Leftover>>,
    broken: AtomicBool,
    resets: AtomicU32,
}

impl Demux {
    fn new(socket: std::net::UdpSocket) -> io::Result<Self> {
        Ok(Self(Arc::new(Shared {
            dual: socket.local_addr()?.is_ipv6(),
            state: UdpSocketState::new((&socket).into())?,
            io: tokio::net::UdpSocket::from_std(socket)?,
            asked: Locked::new(HashMap::new()),
            relays: Locked::new(relay::Routes::default()),
            leftover: Locked::new(None),
            broken: AtomicBool::new(false),
            resets: AtomicU32::new(0),
        })))
    }

    fn broken(&self) -> bool {
        self.0.broken.load(Ordering::Acquire)
    }

    fn send_to(&self, destination: SocketAddr, contents: &[u8]) {
        let destination = match destination {
            SocketAddr::V4(address) if self.0.dual => {
                SocketAddr::new(IpAddr::V6(address.ip().to_ipv6_mapped()), address.port())
            }
            destination => destination,
        };
        drop(self.0.try_send(&Transmit {
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
            self.0.asked.lock().ok()?.insert(id, sender);
            self.send_to(server, &stun_request(id));
            let answered = tokio::time::timeout(STUN_WAIT, answer).await;
            self.0.asked.lock().ok()?.remove(&id);
            if let Ok(Ok(address)) = answered {
                return Some(address);
            }
        }
        None
    }

    /// Whether noq reaches `address` through the relay.
    fn relays(&self, address: SocketAddr) -> bool {
        self.0
            .relays
            .lock()
            .is_ok_and(|routes| routes.bound(address).is_some())
    }

    /// Hands relayed segments on as from the stand-in of the device that sent them.
    fn relayed(&self, buffer: &mut [u8], details: &mut RecvMeta) -> bool {
        let Ok(mut routes) = self.0.relays.lock() else {
            return false;
        };
        let (handed, leftover) =
            routes.unwrap(&mut buffer[..details.len], details.addr, details.stride);
        drop(routes);
        if let Some(leftover) = leftover
            && let Ok(mut kept) = self.0.leftover.lock()
        {
            *kept = Some(leftover);
        }
        let Some((stand_in, length, stride)) = handed else {
            return false;
        };
        details.addr = stand_in;
        details.len = length;
        details.stride = stride;
        details.ecn = None;
        true
    }

    fn absorbed(&self, datagram: &[u8]) -> bool {
        if datagram.first().is_none_or(|first| first & 0x40 != 0) {
            return false;
        }
        if let Some((id, address)) = stun_answer(datagram)
            && let Some(asked) = self
                .0
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

impl Shared {
    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        let relayed = self
            .relays
            .lock()
            .ok()
            .and_then(|routes| routes.wrap(transmit.destination));
        let Some((server, header)) = relayed else {
            return self.io.try_io(Interest::WRITABLE, || {
                self.state.try_send((&self.io).into(), transmit)
            });
        };
        let packet = relay::wrapped(header, transmit.contents, transmit.segment_size);
        let destination = match server {
            SocketAddr::V4(address) if self.dual => {
                SocketAddr::new(IpAddr::V6(address.ip().to_ipv6_mapped()), address.port())
            }
            server => server,
        };
        let wrapped = Transmit {
            destination,
            ecn: None,
            contents: &packet,
            segment_size: relay::wrapped_segment(transmit.segment_size),
            src_ip: None,
        };
        self.io.try_io(Interest::WRITABLE, || {
            self.state.try_send((&self.io).into(), &wrapped)
        })
    }
}

impl AsyncUdpSocket for Demux {
    fn create_sender(&self) -> Pin<Box<dyn UdpSender>> {
        Box::pin(Sender {
            shared: Arc::clone(&self.0),
            writable: None,
        })
    }

    fn poll_recv(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let (Some(buffer), Some(details)) = (bufs.first_mut(), meta.first_mut()) else {
            return Poll::Ready(Ok(0));
        };
        let shared = &self.0;
        loop {
            let leftover = shared
                .leftover
                .lock()
                .ok()
                .and_then(|mut leftover| leftover.take())
                .and_then(|leftover| leftover.into_buffer(buffer));
            if let Some((from, length, stride)) = leftover {
                (details.addr, details.len, details.stride) = (from, length, stride);
                if self.relayed(buffer, details) {
                    return Poll::Ready(Ok(1));
                }
                continue;
            }
            ready!(shared.io.poll_recv_ready(cx))?;
            let read = shared.io.try_io(Interest::READABLE, || {
                shared.state.recv(
                    (&shared.io).into(),
                    std::slice::from_mut(buffer),
                    std::slice::from_mut(details),
                )
            });
            match read {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Ok(0) => return Poll::Ready(Ok(0)),
                Ok(_) => {
                    shared.resets.store(0, Ordering::Relaxed);
                    if matches!(buffer.first(), Some(&(relay::DATA | relay::BOUND))) {
                        if self.relayed(buffer, details) {
                            return Poll::Ready(Ok(1));
                        }
                        continue;
                    }
                    if details.len == details.stride && self.absorbed(&buffer[..details.len]) {
                        continue;
                    }
                    return Poll::Ready(Ok(1));
                }
                // noq retries at once on ConnectionReset and closes the endpoint on other errors,
                // so cap consecutive resets and report them as NotConnected to avoid spinning.
                Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {
                    if shared.resets.fetch_add(1, Ordering::Relaxed) < MOST_RESETS {
                        return Poll::Ready(Err(error));
                    }
                    shared.broken.store(true, Ordering::Release);
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::NotConnected, error)));
                }
                Err(error) => {
                    shared.broken.store(true, Ordering::Release);
                    return Poll::Ready(Err(error));
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.io.local_addr()
    }

    fn max_receive_segments(&self) -> NonZeroUsize {
        self.0.state.gro_segments()
    }

    fn may_fragment(&self) -> bool {
        self.0.state.may_fragment()
    }
}

/// Gives each of noq's tasks its own `writable()` future because Tokio only wakes the last task
/// that polled a socket's readiness directly.
struct Sender {
    shared: Arc<Shared>,
    writable: Option<Pin<Box<dyn Future<Output = io::Result<()>> + Send + Sync>>>,
}

impl fmt::Debug for Sender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl UdpSender for Sender {
    fn poll_send(
        mut self: Pin<&mut Self>,
        transmit: &Transmit<'_>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        loop {
            let writable = this.writable.get_or_insert_with(|| {
                let shared = Arc::clone(&this.shared);
                Box::pin(async move { shared.io.writable().await })
            });
            let ready = ready!(writable.as_mut().poll(cx));
            this.writable = None;
            ready?;
            match this.shared.try_send(transmit) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                // UDP may lose any datagram, and QUIC recovers, as from a loss on the path.
                Err(_) | Ok(()) => return Poll::Ready(Ok(())),
            }
        }
    }

    fn max_transmit_segments(&self) -> NonZeroUsize {
        self.shared.state.max_gso_segments()
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
