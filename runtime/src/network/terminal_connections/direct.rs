use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};

use super::{
    channel::{Channel, Link},
    wire::{DirectOffer, Frame, START_TOKEN},
    *,
};
use crate::identity::keys::DeviceKeys;

const CONNECT: Duration = Duration::from_millis(400);
const START: Duration = Duration::from_secs(5);
const HANDSHAKES: usize = 16;
const OFFERS: usize = 256;
const RETRY_AFTER: Duration = Duration::from_mins(10);

type Token = [u8; START_TOKEN];
type Offers = Arc<std::sync::Mutex<BTreeMap<Token, Offer>>>;

struct Offer {
    key: Vec<u8>,
    deliver: oneshot::Sender<Channel<Link>>,
}

struct Listening {
    address: SocketAddr,
    key: Vec<u8>,
    stop: CancellationToken,
}

pub(crate) struct Direct {
    enabled: bool,
    port: u16,
    offers: Offers,
    listening: Mutex<Option<Listening>>,
    unreachable: std::sync::Mutex<BTreeMap<SocketAddr, tokio::time::Instant>>,
}

pub(super) struct Offered {
    token: Token,
    offers: Offers,
    pub(super) channel: oneshot::Receiver<Channel<Link>>,
}

impl Drop for Offered {
    fn drop(&mut self) {
        if let Ok(mut offers) = self.offers.lock() {
            offers.remove(&self.token);
        }
    }
}

impl Direct {
    pub(crate) fn new(enabled: bool, port: u16) -> Self {
        Self {
            enabled,
            port,
            offers: Offers::default(),
            listening: Mutex::new(None),
            unreachable: std::sync::Mutex::default(),
        }
    }

    pub(super) async fn offer(
        &self,
        keys: &Arc<DeviceKeys>,
        viewer_key: &[u8],
        stop: &CancellationToken,
    ) -> Option<(DirectOffer, Offered)> {
        if !self.enabled {
            return None;
        }
        self.offer_at(local_address()?, keys, viewer_key, stop)
            .await
    }

    async fn offer_at(
        &self,
        local: Ipv4Addr,
        keys: &Arc<DeviceKeys>,
        viewer_key: &[u8],
        stop: &CancellationToken,
    ) -> Option<(DirectOffer, Offered)> {
        let mut listening = self.listening.lock().await;
        if listening.as_ref().is_none_or(|current| {
            current.stop.is_cancelled()
                || current.address.ip() != IpAddr::V4(local)
                || current.key != keys.signing_public()
        }) {
            if let Some(old) = listening.take() {
                old.stop.cancel();
            }
            *listening = listen(local, self.port, keys, &self.offers, stop.child_token()).await;
        }
        let address = listening.as_ref()?.address;
        drop(listening);
        let mut token = Token::default();
        aws_lc_rs::rand::fill(&mut token).ok()?;
        let (deliver, channel) = oneshot::channel();
        {
            let mut offers = self.offers.lock().ok()?;
            if offers.len() >= OFFERS {
                return None;
            }
            offers.insert(
                token,
                Offer {
                    key: viewer_key.to_vec(),
                    deliver,
                },
            );
        }
        Some((
            DirectOffer {
                token: hex(&token),
                addresses: vec![address.to_string()],
            },
            Offered {
                token,
                offers: Arc::clone(&self.offers),
                channel,
            },
        ))
    }

    pub(super) async fn connect(
        &self,
        offer: &DirectOffer,
        keys: &DeviceKeys,
        host_key: &[u8],
    ) -> Option<(Channel<Link>, SocketAddr)> {
        if !self.enabled {
            return None;
        }
        let token = token(&offer.token)?;
        let local = local_address()?;
        let address = offer
            .addresses
            .iter()
            .take(4)
            .filter_map(|address| address.parse().ok())
            .find(|address| near(local, address))?;
        if self.failed_recently(&address) {
            return None;
        }
        let channel = reach(address, token, keys, host_key).await;
        if channel.is_none() {
            self.failed(address);
        }
        channel.map(|channel| (channel, address))
    }

    fn failed_recently(&self, address: &SocketAddr) -> bool {
        self.unreachable.lock().is_ok_and(|unreachable| {
            unreachable
                .get(address)
                .is_some_and(|at| at.elapsed() < RETRY_AFTER)
        })
    }

    pub(super) fn failed(&self, address: SocketAddr) {
        if let Ok(mut unreachable) = self.unreachable.lock() {
            unreachable.retain(|_, at| at.elapsed() < RETRY_AFTER);
            unreachable.insert(address, tokio::time::Instant::now());
        }
    }
}

async fn listen(
    local: Ipv4Addr,
    port: u16,
    keys: &Arc<DeviceKeys>,
    offers: &Offers,
    stop: CancellationToken,
) -> Option<Listening> {
    let listener = match TcpListener::bind((local, port)).await {
        Ok(listener) => listener,
        Err(_) => TcpListener::bind((local, 0)).await.ok()?,
    };
    let address = listener.local_addr().ok()?;
    let (keys, offers, accepting) = (Arc::clone(keys), Arc::clone(offers), stop.clone());
    let key = keys.signing_public().to_vec();
    tokio::spawn(async move {
        let handshakes = Arc::new(Semaphore::new(HANDSHAKES));
        loop {
            let accepted = tokio::select! {
                () = accepting.cancelled() => return,
                accepted = listener.accept() => accepted,
            };
            let Ok((stream, _)) = accepted else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let Ok(permit) = Arc::clone(&handshakes).try_acquire_owned() else {
                continue;
            };
            let (keys, offers) = (Arc::clone(&keys), Arc::clone(&offers));
            tokio::spawn(async move {
                let _permit = permit;
                let _outcome = tokio::time::timeout(START, admit(stream, &keys, &offers)).await;
            });
        }
    });
    Some(Listening { address, key, stop })
}

async fn admit(stream: TcpStream, keys: &DeviceKeys, offers: &Offers) -> Result<()> {
    stream.set_nodelay(true)?;
    let mut channel = Channel::accept(Link::Direct(stream), keys.signing_pkcs8()).await?;
    let Some((Frame::Start { token }, _)) = channel.receive().await? else {
        return Err(Error::Closed);
    };
    let token = Token::try_from(&token[..]).map_err(|_| Error::Closed)?;
    let key = channel.peer_key()?;
    let offer = {
        let mut offers = offers.lock().map_err(|_| Error::Closed)?;
        if offers.get(&token).is_none_or(|offer| offer.key != key) {
            return Err(Error::Closed);
        }
        offers.remove(&token)
    };
    if let Some(offer) = offer {
        drop(offer.deliver.send(channel));
    }
    Ok(())
}

async fn reach(
    address: SocketAddr,
    token: Token,
    keys: &DeviceKeys,
    host_key: &[u8],
) -> Option<Channel<Link>> {
    let stream = tokio::time::timeout(CONNECT, TcpStream::connect(address))
        .await
        .ok()?
        .ok()?;
    stream.set_nodelay(true).ok()?;
    let mut channel = tokio::time::timeout(
        START,
        Channel::connect(Link::Direct(stream), keys.signing_pkcs8(), host_key),
    )
    .await
    .ok()?
    .ok()?;
    channel
        .send(&Frame::Start {
            token: Bytes::copy_from_slice(&token),
        })
        .await
        .ok()?;
    Some(channel)
}

fn local_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(local) if local.is_private() => Some(local),
        _ => None,
    }
}

fn near(local: Ipv4Addr, candidate: &SocketAddr) -> bool {
    match candidate.ip() {
        IpAddr::V4(address) => {
            candidate.port() != 0
                && address.is_private()
                && address.octets()[..3] == local.octets()[..3]
        }
        IpAddr::V6(_) => false,
    }
}

fn hex(token: &Token) -> String {
    use std::fmt::Write as _;
    token
        .iter()
        .fold(String::with_capacity(2 * START_TOKEN), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn token(text: &str) -> Option<Token> {
    if text.len() != 2 * START_TOKEN || !text.is_ascii() {
        return None;
    }
    let mut token = Token::default();
    for (byte, pair) in token.iter_mut().zip(text.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Arc<DeviceKeys> {
        Arc::new(DeviceKeys::generate().unwrap())
    }

    #[tokio::test]
    async fn a_viewer_with_the_token_and_the_admitted_key_reaches_the_host_and_no_other_device_does()
     {
        let (host, viewer, other) = (keys(), keys(), keys());
        let taken = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let direct = Direct::new(true, taken.local_addr().unwrap().port());
        let stop = CancellationToken::new();
        let (offer, mut offered) = direct
            .offer_at(Ipv4Addr::LOCALHOST, &host, viewer.signing_public(), &stop)
            .await
            .unwrap();
        let address: SocketAddr = offer.addresses[0].parse().unwrap();
        assert_ne!(address.port(), taken.local_addr().unwrap().port());
        let secret = token(&offer.token).unwrap();

        let mut wrong_key = reach(address, secret, &other, host.signing_public())
            .await
            .unwrap();
        assert!(!matches!(wrong_key.receive().await, Ok(Some(_))));
        let mut wrong_token = reach(address, Token::default(), &viewer, host.signing_public())
            .await
            .unwrap();
        assert!(!matches!(wrong_token.receive().await, Ok(Some(_))));
        assert!(offered.channel.try_recv().is_err());
        assert!(
            reach(address, secret, &viewer, other.signing_public())
                .await
                .is_none()
        );

        let mut view = reach(address, secret, &viewer, host.signing_public())
            .await
            .unwrap();
        let mut hosted = tokio::time::timeout(START, &mut offered.channel)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hosted.peer_key().unwrap(), viewer.signing_public());
        hosted.send(&Frame::Refresh).await.unwrap();
        assert!(matches!(
            view.receive().await,
            Ok(Some((Frame::Refresh, _)))
        ));
        view.send(&Frame::Ack { received: 7 }).await.unwrap();
        assert!(matches!(
            hosted.receive().await,
            Ok(Some((Frame::Ack { received: 7 }, _)))
        ));

        drop(offered);
        assert!(direct.offers.lock().unwrap().is_empty());
        stop.cancel();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            tokio::time::timeout(CONNECT, TcpStream::connect(address))
                .await
                .is_ok_and(|connected| connected.is_err())
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_address_that_failed_is_not_tried_again_for_ten_minutes() {
        let direct = Direct::new(true, 0);
        let address: SocketAddr = "192.168.1.7:4100".parse().unwrap();
        assert!(!direct.failed_recently(&address));
        direct.failed(address);
        tokio::time::advance(RETRY_AFTER / 2).await;
        assert!(direct.failed_recently(&address));
        assert!(!direct.failed_recently(&"192.168.1.7:4101".parse().unwrap()));
        tokio::time::advance(RETRY_AFTER / 2).await;
        assert!(!direct.failed_recently(&address));
    }

    #[test]
    fn a_viewer_tries_only_a_private_address_of_its_own_network() {
        let local = Ipv4Addr::new(192, 168, 1, 20);
        let near_to = |text: &str| near(local, &text.parse().unwrap());
        assert!(near_to("192.168.1.7:4100"));
        assert!(near_to("192.168.1.20:4100"));
        assert!(!near_to("192.168.2.7:4100"));
        assert!(!near_to("192.168.1.7:0"));
        assert!(!near_to("[fe80::1]:4100"));
        assert!(!near(
            Ipv4Addr::new(8, 8, 8, 20),
            &"8.8.8.8:4100".parse().unwrap()
        ));
        assert_eq!(token(&hex(&[7; START_TOKEN])), Some([7; START_TOKEN]));
        assert_eq!(token("07"), None);
    }
}
