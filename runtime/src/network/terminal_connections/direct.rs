#[cfg(all(feature = "host", not(test)))]
use std::net::IpAddr;
#[cfg(feature = "host")]
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use futures_util::stream::FuturesUnordered;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
#[cfg(feature = "host")]
use tokio::{net::TcpListener, sync::Semaphore};

use super::{
    RemoteRequest,
    channel::{Channel, ChannelKeys, Transport},
    wire::Frame,
    *,
};
use crate::canvas::{BROWSER_PIPE, MOST_PIPE_FRAME, PipeEnds};

const READ_BYTES: usize = 64 * 1024;
const SEND: Duration = Duration::from_secs(10);
const ANSWER: Duration = Duration::from_secs(1);
const TRYING: Duration = Duration::from_millis(500);
const ALIVE: Duration = Duration::from_secs(5);
const SILENCE: Duration = Duration::from_secs(15);
#[cfg(feature = "host")]
const LISTENING: Duration = Duration::from_secs(5);
#[cfg(feature = "host")]
const OPENED: Duration = Duration::from_secs(5);
#[cfg(feature = "host")]
const PORTS: std::ops::RangeInclusive<u16> = 47360..=47379;
#[cfg(feature = "host")]
const STRAIGHT_PIPES: usize = 2;
#[cfg(feature = "host")]
const LISTEN_ON: Ipv4Addr = if cfg!(test) {
    Ipv4Addr::LOCALHOST
} else {
    Ipv4Addr::UNSPECIFIED
};

pub(super) struct Tcp(TcpStream);

impl Transport for Tcp {
    async fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        tokio::time::timeout(SEND, self.0.write_all(&bytes))
            .await
            .map_err(|_| Error::Closed)?
            .map_err(|_| Error::Closed)
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        let mut bytes = vec![0; READ_BYTES];
        let read = self.0.read(&mut bytes).await.map_err(|_| Error::Closed)?;
        bytes.truncate(read);
        Ok((read > 0).then_some(bytes))
    }
}

/// The host of a terminal, as the terminal's channel proved it.
pub(crate) struct Host {
    keys: Arc<ChannelKeys>,
    device: String,
    key: Vec<u8>,
}

impl Host {
    pub(super) const fn new(keys: Arc<ChannelKeys>, device: String, key: Vec<u8>) -> Self {
        Self { keys, device, key }
    }

    /// The first of its addresses that answers with the key of the host.
    async fn reach(&self, addresses: &[SocketAddr]) -> Option<Channel<Tcp>> {
        let mut tries = addresses
            .iter()
            .map(|address| async move {
                let stream = TcpStream::connect(address).await.ok()?;
                stream.set_nodelay(true).ok()?;
                Channel::connect(Tcp(stream), &self.keys, &self.device, &self.key)
                    .await
                    .ok()
                    .filter(|channel| channel.version().is_ok())
            })
            .collect::<FuturesUnordered<_>>();
        tokio::time::timeout(TRYING, async {
            while let Some(tried) = tries.next().await {
                if tried.is_some() {
                    return tried;
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }
}

/// What a pipe of a viewer needs to reach the host.
pub(super) struct Pipe {
    pub(super) commands: mpsc::Sender<RemoteRequest>,
    pub(super) cancel: CancellationToken,
    pub(super) id: Uuid,
    pub(super) key: String,
    pub(super) name: String,
    pub(super) to_view: mpsc::Sender<Vec<u8>>,
    pub(super) from_view: mpsc::Receiver<Vec<u8>>,
}

/// Carries a pipe of a viewer. The live picture of a browser goes straight to the host when the
/// viewer can reach it, and otherwise on the terminal's channel. The path stays as it is until the
/// pipe ends; the view then joins again.
pub(super) async fn run(mut pipe: Pipe) {
    if pipe.key == BROWSER_PIPE
        && let Some(mut channel) = ask(&pipe.commands, pipe.id).await
    {
        let opened = Frame::PipeOpen {
            pipe: pipe.id,
            key: pipe.key,
            name: pipe.name,
        };
        if channel.send(&opened).await.is_ok() {
            let ends = PipeEnds {
                sender: pipe.to_view,
                receiver: pipe.from_view,
            };
            tokio::select! {
                () = pipe.cancel.cancelled() => {}
                carried = carry(&mut channel, pipe.id, ends) => {
                    if let Err(error) = carried {
                        tracing::debug!(%error, "a straight connection to a host ended");
                    }
                }
            }
            channel.close().await;
        }
        return;
    }
    let opened = RemoteRequest::PipeOpen {
        pipe: pipe.id,
        key: pipe.key,
        name: pipe.name,
        frames: pipe.to_view,
    };
    if pipe.commands.send(opened).await.is_err() {
        return;
    }
    while let Some(frame) = pipe.from_view.recv().await {
        let parts = frame.len().div_ceil(wire::PIPE_PART);
        for (index, part) in frame.chunks(wire::PIPE_PART).enumerate() {
            let data = RemoteRequest::PipeData {
                pipe: pipe.id,
                more: index + 1 < parts,
                part: Bytes::copy_from_slice(part),
            };
            if pipe.commands.send(data).await.is_err() {
                return;
            }
        }
    }
    drop(
        pipe.commands
            .send(RemoteRequest::PipeClose { pipe: pipe.id })
            .await,
    );
}

async fn ask(commands: &mpsc::Sender<RemoteRequest>, pipe: Uuid) -> Option<Channel<Tcp>> {
    let (reply, offer) = oneshot::channel();
    commands
        .send(RemoteRequest::DirectAsk { pipe, reply })
        .await
        .ok()?;
    let (host, addresses) = tokio::time::timeout(ANSWER, offer).await.ok()?.ok()?;
    host.reach(&addresses).await
}

/// Carries one pipe in both directions until a side ends it. A view that falls behind loses its
/// pipe and joins again.
async fn carry<T: Transport>(channel: &mut Channel<T>, pipe: Uuid, ends: PipeEnds) -> Result<()> {
    let PipeEnds {
        sender,
        mut receiver,
    } = ends;
    let mut filling = Vec::new();
    let mut alive = tokio::time::interval(ALIVE);
    alive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heard = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = alive.tick() => {
                if heard.elapsed() >= SILENCE {
                    return Err(Error::Closed);
                }
                channel.send(&Frame::Heartbeat { number: 0, next_sequence: 0 }).await?;
            }
            frame = channel.receive() => {
                heard = tokio::time::Instant::now();
                match frame? {
                    Some((Frame::Heartbeat { .. }, _)) => {}
                    Some((Frame::PipeData { more, part, .. }, _)) => {
                        if filling.len() + part.len() > MOST_PIPE_FRAME {
                            return Err(invalid("A document frame exceeds its bound."));
                        }
                        filling.extend_from_slice(&part);
                        if !more && sender.try_send(std::mem::take(&mut filling)).is_err() {
                            break;
                        }
                    }
                    Some((Frame::PipeClose { .. }, _)) | None => return Ok(()),
                    Some(_) => return Err(invalid("Unsupported frame on a straight connection.")),
                }
            }
            frame = receiver.recv() => {
                let Some(frame) = frame else { break };
                let parts = frame.len().div_ceil(wire::PIPE_PART);
                for (index, part) in frame.chunks(wire::PIPE_PART).enumerate() {
                    channel
                        .send(&Frame::PipeData {
                            pipe,
                            more: index + 1 < parts,
                            part: Bytes::copy_from_slice(part),
                        })
                        .await?;
                }
            }
        }
    }
    channel.send(&Frame::PipeClose { pipe }).await?;
    Ok(())
}

/// How one viewer of a terminal gets a straight connection to this host. The port opens only when
/// the viewer asks, for a short time, and only the device of this viewer gets through.
#[cfg(feature = "host")]
pub(super) struct Door {
    keys: Arc<ChannelKeys>,
    viewer: Vec<u8>,
    requests: mpsc::Sender<HostRequest>,
    connection: Uuid,
    authorization: CancellationToken,
    pipes: Arc<Semaphore>,
}

#[cfg(feature = "host")]
impl Door {
    pub(super) fn new(
        keys: Arc<ChannelKeys>,
        viewer: Vec<u8>,
        requests: mpsc::Sender<HostRequest>,
        connection: Uuid,
        authorization: CancellationToken,
    ) -> Self {
        Self {
            keys,
            viewer,
            requests,
            connection,
            authorization,
            pipes: Arc::new(Semaphore::new(STRAIGHT_PIPES)),
        }
    }

    /// Says where the viewer can try. No answer sends the viewer to the terminal's channel.
    pub(super) async fn open(&self) -> Vec<SocketAddr> {
        let Ok(room) = Arc::clone(&self.pipes).try_acquire_owned() else {
            return Vec::new();
        };
        let Some(listener) = listen().await else {
            return Vec::new();
        };
        let Ok(port) = listener.local_addr().map(|address| address.port()) else {
            return Vec::new();
        };
        let keys = Arc::clone(&self.keys);
        let viewer = self.viewer.clone();
        let requests = self.requests.clone();
        let connection = self.connection;
        let authorization = self.authorization.clone();
        tokio::spawn(async move {
            let _room = room;
            let welcomed = tokio::select! {
                () = authorization.cancelled() => None,
                welcomed = tokio::time::timeout(LISTENING, welcome(&listener, &keys, &viewer)) => welcomed.ok().flatten(),
            };
            drop(listener);
            let Some(mut channel) = welcomed else { return };
            tokio::select! {
                () = authorization.cancelled() => {}
                served = serve(&mut channel, &requests, connection) => {
                    if let Err(error) = served {
                        tracing::debug!(%error, "a straight connection of a viewer ended");
                    }
                }
            }
            channel.close().await;
        });
        own_addresses()
            .into_iter()
            .map(|address| SocketAddr::from((address, port)))
            .collect()
    }
}

#[cfg(feature = "host")]
async fn listen() -> Option<TcpListener> {
    for port in PORTS {
        if let Ok(listener) = TcpListener::bind((LISTEN_ON, port)).await {
            return Some(listener);
        }
    }
    None
}

#[cfg(all(feature = "host", not(test)))]
fn own_addresses() -> Vec<Ipv4Addr> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    interfaces
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(address) if !address.is_loopback() && !address.is_link_local() => {
                Some(address)
            }
            _ => None,
        })
        .take(wire::MOST_DIRECT_ADDRESSES)
        .collect()
}

#[cfg(all(feature = "host", test))]
fn own_addresses() -> Vec<Ipv4Addr> {
    vec![Ipv4Addr::LOCALHOST]
}

/// Takes connections until one proves the key of the viewer.
#[cfg(feature = "host")]
async fn welcome(
    listener: &TcpListener,
    keys: &ChannelKeys,
    viewer: &[u8],
) -> Option<Channel<Tcp>> {
    loop {
        let (stream, _) = listener.accept().await.ok()?;
        stream.set_nodelay(true).ok()?;
        if let Ok(channel) = Channel::accept(Tcp(stream), keys).await
            && channel.peer_key().is_ok_and(|key| key == viewer)
            && channel.version().is_ok()
        {
            return Some(channel);
        }
    }
}

#[cfg(feature = "host")]
async fn serve(
    channel: &mut Channel<Tcp>,
    requests: &mpsc::Sender<HostRequest>,
    connection_id: Uuid,
) -> Result<()> {
    let opened = tokio::time::timeout(OPENED, channel.receive())
        .await
        .map_err(|_| Error::Closed)??;
    let Some((Frame::PipeOpen { pipe, key, name }, _)) = opened else {
        return Err(invalid("A straight connection opens with its pipe."));
    };
    let (reply, response) = oneshot::channel();
    requests
        .send(HostRequest::Document {
            connection_id,
            key,
            name,
            reply,
        })
        .await
        .map_err(|_| Error::Closed)?;
    if let Ok(Ok(ends)) = response.await {
        return carry(channel, pipe, ends).await;
    }
    channel.send(&Frame::PipeClose { pipe }).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
