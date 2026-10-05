use std::{
    io::{Read as _, Write as _},
    sync::Arc,
    time::Duration,
};

use futures_util::{SinkExt, StreamExt as _};
use rustls::{
    ClientConfig, ClientConnection, Connection, DigitallySignedStruct, DistinguishedName,
    HandshakeKind, ServerConfig, ServerConnection, SignatureScheme,
    client::{
        AlwaysResolvesClientRawPublicKeys, Resumption,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    crypto::{
        CryptoProvider, WebPkiSupportedAlgorithms, aws_lc_rs as provider,
        verify_tls13_signature_with_raw_key,
    },
    pki_types::{
        CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, SubjectPublicKeyInfoDer,
        UnixTime,
    },
    server::{
        AlwaysResolvesServerRawPublicKeys, ServerSessionMemoryCache,
        danger::{ClientCertVerified, ClientCertVerifier},
    },
    sign::CertifiedKey,
    version::TLS13,
};
use tokio_tungstenite::tungstenite::Message;

use super::{Socket, wire::Frame};
use crate::network::{Error, Result, invalid};

const HANDSHAKE: Duration = Duration::from_secs(10);
const SEND: Duration = Duration::from_secs(10);
const MESSAGE_BYTES: usize = 64 * 1024;
const KEY_FRAMES: u64 = 1 << 24;
const KEY_AGE: Duration = Duration::from_hours(1);
const DEVICE_KEY_BYTES: usize = 1952;
const RESUMABLE_HOSTS: usize = 256;
const RESUMABLE_VIEWS: usize = 1024;

pub(super) trait Transport: Send {
    fn send(&mut self, bytes: Vec<u8>) -> impl Future<Output = Result<()>> + Send;
    fn receive(&mut self) -> impl Future<Output = Result<Option<Vec<u8>>>> + Send;
}

impl Transport for Socket {
    async fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        tokio::time::timeout(SEND, SinkExt::send(self, Message::Binary(bytes.into())))
            .await
            .map_err(|_| Error::Closed)?
            .map_err(|_| Error::Closed)
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            match self.next().await {
                Some(Ok(Message::Binary(bytes))) => return Ok(Some(bytes.into())),
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) | None => return Ok(None),
                Some(Ok(_)) => return Err(invalid("Unexpected terminal channel message.")),
                Some(Err(_)) => return Err(Error::Closed),
            }
        }
    }
}

pub(super) struct Channel<T> {
    transport: T,
    tls: Connection,
    plain: Vec<u8>,
    closed: bool,
    frames: u64,
    keyed: tokio::time::Instant,
}

pub(crate) struct ChannelKeys {
    public: Vec<u8>,
    client: Arc<ClientConfig>,
    server: Arc<ServerConfig>,
}

impl ChannelKeys {
    pub(crate) fn new(signing_pkcs8: &[u8], signing_public: &[u8]) -> Result<Self> {
        let provider = provider();
        let own = certified(&provider, signing_pkcs8)?;
        let mut client = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[&TLS13])
            .map_err(tls)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(DeviceKey {
                algorithms: provider.signature_verification_algorithms,
            }))
            .with_client_cert_resolver(Arc::new(AlwaysResolvesClientRawPublicKeys::new(
                Arc::clone(&own),
            )));
        client.resumption = Resumption::in_memory_sessions(RESUMABLE_HOSTS);
        client.enable_sni = false;
        let mut server = ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[&TLS13])
            .map_err(tls)?
            .with_client_cert_verifier(Arc::new(DeviceKey {
                algorithms: provider.signature_verification_algorithms,
            }))
            .with_cert_resolver(Arc::new(AlwaysResolvesServerRawPublicKeys::new(own)));
        server.session_storage = ServerSessionMemoryCache::new(RESUMABLE_VIEWS);
        server.send_tls13_tickets = 2;
        server.send_half_rtt_data = true;
        Ok(Self {
            public: signing_public.to_vec(),
            client: Arc::new(client),
            server: Arc::new(server),
        })
    }

    pub(crate) fn belongs_to(&self, signing_public: &[u8]) -> bool {
        self.public == signing_public
    }
}

impl<T: Transport> Channel<T> {
    pub(super) async fn connect(
        transport: T,
        keys: &ChannelKeys,
        host_device: &str,
        host_key: &[u8],
    ) -> Result<Self> {
        let name = ServerName::try_from(format!("{host_device}.kodosi.invalid"))
            .map_err(|_| invalid("The hosting device identity is not valid."))?;
        let tls = ClientConnection::new(Arc::clone(&keys.client), name).map_err(tls)?;
        let channel = Self::establish(transport, Connection::Client(tls)).await?;
        if channel.peer_key()? != host_key {
            return Err(Error::Trust(
                "The hosting device key is not the approved key.".into(),
            ));
        }
        Ok(channel)
    }

    pub(super) async fn accept(transport: T, keys: &ChannelKeys) -> Result<Self> {
        let tls = ServerConnection::new(Arc::clone(&keys.server)).map_err(tls)?;
        Self::establish(transport, Connection::Server(tls)).await
    }

    pub(super) fn resumed(&self) -> bool {
        self.tls.handshake_kind() == Some(HandshakeKind::Resumed)
    }

    async fn establish(transport: T, mut tls: Connection) -> Result<Self> {
        tls.set_buffer_limit(None);
        let mut channel = Self {
            transport,
            tls,
            plain: Vec::new(),
            closed: false,
            frames: 0,
            keyed: tokio::time::Instant::now(),
        };
        tokio::time::timeout(HANDSHAKE, channel.handshake())
            .await
            .map_err(|_| Error::Closed)??;
        Ok(channel)
    }

    async fn handshake(&mut self) -> Result<()> {
        loop {
            self.flush().await?;
            let resumed_host = matches!(self.tls, Connection::Server(_)) && self.resumed();
            if !self.tls.is_handshaking() || resumed_host {
                return Ok(());
            }
            let message = self.transport.receive().await?.ok_or(Error::Closed)?;
            self.absorb(&message)?;
        }
    }

    pub(super) fn peer_key(&self) -> Result<Vec<u8>> {
        let info = self
            .tls
            .peer_certificates()
            .and_then(<[CertificateDer<'_>]>::first)
            .ok_or_else(|| Error::Trust("The other device did not prove its key.".into()))?;
        let info = info.as_ref();
        if info.len() <= DEVICE_KEY_BYTES {
            return Err(Error::Trust("The other device key is not valid.".into()));
        }
        Ok(info[info.len() - DEVICE_KEY_BYTES..].to_vec())
    }

    pub(super) async fn send(&mut self, frame: &Frame) -> Result<usize> {
        let encoded = frame.encode()?;
        self.frames += 1;
        if self.frames >= KEY_FRAMES || self.keyed.elapsed() >= KEY_AGE {
            self.tls.refresh_traffic_keys().map_err(tls)?;
            self.frames = 0;
            self.keyed = tokio::time::Instant::now();
        }
        self.tls
            .writer()
            .write_all(&encoded)
            .map_err(|_| Error::Closed)?;
        self.flush().await?;
        Ok(encoded.len())
    }

    pub(super) async fn receive(&mut self) -> Result<Option<(Frame, usize)>> {
        loop {
            if let Some(frame) = Frame::take(&mut self.plain)? {
                return Ok(Some(frame));
            }
            if self.closed {
                return Ok(None);
            }
            let Some(message) = self.transport.receive().await? else {
                return Ok(None);
            };
            self.absorb(&message)?;
            self.flush().await?;
        }
    }

    pub(super) async fn close(mut self) {
        self.tls.send_close_notify();
        let _outcome = tokio::time::timeout(Duration::from_secs(2), self.flush()).await;
    }

    fn absorb(&mut self, mut bytes: &[u8]) -> Result<()> {
        if bytes.len() > MESSAGE_BYTES * 2 {
            return Err(invalid("Terminal channel message exceeds its bound."));
        }
        while !bytes.is_empty() {
            if self.tls.read_tls(&mut bytes).map_err(|_| Error::Closed)? == 0 {
                return Err(invalid("Terminal channel data could not be read."));
            }
            let state = self.tls.process_new_packets().map_err(tls)?;
            let available = state.plaintext_bytes_to_read();
            if available > 0 {
                let start = self.plain.len();
                self.plain.resize(start + available, 0);
                self.tls
                    .reader()
                    .read_exact(&mut self.plain[start..])
                    .map_err(|_| Error::Closed)?;
            }
            self.closed |= state.peer_has_closed();
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        while self.tls.wants_write() {
            let mut wire = Vec::with_capacity(MESSAGE_BYTES);
            while self.tls.wants_write() && wire.len() < MESSAGE_BYTES {
                self.tls.write_tls(&mut wire).map_err(|_| Error::Closed)?;
            }
            self.transport.send(wire).await?;
        }
        Ok(())
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(CryptoProvider {
        kx_groups: vec![provider::kx_group::X25519MLKEM768],
        cipher_suites: vec![provider::cipher_suite::TLS13_AES_256_GCM_SHA384],
        ..provider::default_provider()
    })
}

fn certified(provider: &CryptoProvider, signing_pkcs8: &[u8]) -> Result<Arc<CertifiedKey>> {
    let key = provider
        .key_provider
        .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            signing_pkcs8.to_vec(),
        )))
        .map_err(|_| invalid("Stored device signing key is invalid."))?;
    let info = key
        .public_key()
        .ok_or_else(|| invalid("Stored device signing key is invalid."))?
        .as_ref()
        .to_vec();
    Ok(Arc::new(CertifiedKey::new(
        vec![CertificateDer::from(info)],
        key,
    )))
}

fn tls(error: rustls::Error) -> Error {
    match error {
        rustls::Error::General(reason) => Error::Trust(reason),
        rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented => {
            Error::Trust("The other device did not prove its key.".into())
        }
        _ => Error::Closed,
    }
}

#[derive(Debug)]
struct DeviceKey {
    algorithms: WebPkiSupportedAlgorithms,
}

impl DeviceKey {
    fn signature(
        &self,
        message: &[u8],
        presented: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature_with_raw_key(
            message,
            &SubjectPublicKeyInfoDer::from(presented.as_ref()),
            signature,
            &self.algorithms,
        )
    }
}

impl ServerCertVerifier for DeviceKey {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not used.".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.signature(message, cert, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ML_DSA_65]
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

impl ClientCertVerifier for DeviceKey {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not used.".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.signature(message, cert, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ML_DSA_65]
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub(super) mod tests {
    use bytes::Bytes;
    use tokio::sync::mpsc;

    pub(in crate::network::terminal_connections) fn noise(length: usize) -> Vec<u8> {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        (0..length)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                state.to_be_bytes()[0]
            })
            .collect()
    }

    use super::*;
    use crate::identity::keys::DeviceKeys;

    pub(in crate::network::terminal_connections) struct Pipe {
        pub(in crate::network::terminal_connections) outgoing: mpsc::Sender<Vec<u8>>,
        pub(in crate::network::terminal_connections) incoming: mpsc::Receiver<Vec<u8>>,
        pub(in crate::network::terminal_connections) change: Option<fn(&mut Vec<u8>)>,
    }

    impl Transport for Pipe {
        async fn send(&mut self, mut bytes: Vec<u8>) -> Result<()> {
            if let Some(change) = self.change {
                change(&mut bytes);
            }
            self.outgoing.send(bytes).await.map_err(|_| Error::Closed)
        }

        async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
            Ok(self.incoming.recv().await)
        }
    }

    pub(in crate::network::terminal_connections) fn pipes() -> (Pipe, Pipe) {
        let (left, right_incoming) = mpsc::channel(64);
        let (right, left_incoming) = mpsc::channel(64);
        (
            Pipe {
                outgoing: left,
                incoming: left_incoming,
                change: None,
            },
            Pipe {
                outgoing: right,
                incoming: right_incoming,
                change: None,
            },
        )
    }

    pub(in crate::network::terminal_connections) fn channel_keys(keys: &DeviceKeys) -> ChannelKeys {
        ChannelKeys::new(keys.signing_pkcs8(), keys.signing_public()).unwrap()
    }

    pub(in crate::network::terminal_connections) async fn pair(
        host: &DeviceKeys,
        viewer: &DeviceKeys,
    ) -> (Channel<Pipe>, Channel<Pipe>) {
        join(
            &channel_keys(host),
            &channel_keys(viewer),
            host.signing_public(),
        )
        .await
    }

    async fn join(
        host: &ChannelKeys,
        viewer: &ChannelKeys,
        host_key: &[u8],
    ) -> (Channel<Pipe>, Channel<Pipe>) {
        let (near, far) = pipes();
        let (accepted, connected) = tokio::join!(
            Channel::accept(far, host),
            Channel::connect(near, viewer, "host-device", host_key),
        );
        (connected.unwrap(), accepted.unwrap())
    }

    #[tokio::test]
    async fn a_second_channel_of_two_devices_resumes_and_the_host_sends_before_the_viewer_ends_the_handshake()
     {
        let (host, viewer) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let (host_keys, viewer_keys) = (channel_keys(&host), channel_keys(&viewer));
        let (mut near, mut far) = join(&host_keys, &viewer_keys, host.signing_public()).await;
        assert!(!near.resumed() && !far.resumed());
        far.send(&Frame::Refresh).await.unwrap();
        assert!(matches!(
            near.receive().await,
            Ok(Some((Frame::Refresh, _)))
        ));
        drop((near, far));

        let (near, far) = pipes();
        let accepting = Channel::accept(far, &host_keys);
        let connecting = Channel::connect(near, &viewer_keys, "host-device", host.signing_public());
        tokio::pin!(accepting, connecting);
        let mut hosted = tokio::select! {
            accepted = &mut accepting => accepted.unwrap(),
            _ = &mut connecting => panic!("the viewer ended the handshake first"),
        };
        assert!(hosted.resumed());
        assert_eq!(hosted.peer_key().unwrap(), viewer.signing_public());
        hosted.send(&Frame::Ack { received: 9 }).await.unwrap();
        let mut view = connecting.await.unwrap();
        assert!(view.resumed());
        assert_eq!(view.peer_key().unwrap(), host.signing_public());
        assert!(matches!(
            view.receive().await,
            Ok(Some((Frame::Ack { received: 9 }, _)))
        ));
        view.send(&Frame::Refresh).await.unwrap();
        assert!(matches!(
            hosted.receive().await,
            Ok(Some((Frame::Refresh, _)))
        ));

        let other = channel_keys(&DeviceKeys::generate().unwrap());
        let (near, far) = pipes();
        let (accepted, connected) = tokio::join!(
            Channel::accept(far, &other),
            Channel::connect(near, &viewer_keys, "host-device", host.signing_public()),
        );
        assert!(matches!(connected, Err(Error::Trust(_))));
        drop(accepted);
    }

    #[tokio::test]
    async fn two_devices_prove_their_keys_and_exchange_frames_in_order() {
        let (host, viewer) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let (mut near, mut far) = pair(&host, &viewer).await;
        assert_eq!(far.peer_key().unwrap(), viewer.signing_public());
        assert_eq!(near.peer_key().unwrap(), host.signing_public());

        near.send(&Frame::Input {
            offset: 0,
            heartbeat: 0,
            bytes: Bytes::from_static(b"ls\r"),
        })
        .await
        .unwrap();
        near.send(&Frame::Refresh).await.unwrap();
        assert!(matches!(
            far.receive().await.unwrap(),
            Some((Frame::Input { offset: 0, .. }, 64))
        ));
        assert!(matches!(
            far.receive().await.unwrap(),
            Some((Frame::Refresh, 64))
        ));

        let large = Frame::Keyframe {
            next_sequence: 9,
            more: false,
            part: Bytes::from(vec![7; 30_000]),
        };
        let sent = far.send(&large).await.unwrap();
        let Some((Frame::Keyframe { part, .. }, received)) = near.receive().await.unwrap() else {
            panic!("keyframe");
        };
        assert_eq!((part.len(), received), (30_000, sent));

        far.close().await;
        assert!(near.receive().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_viewer_refuses_a_host_that_has_another_key() {
        let (host, viewer, other) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let (host_keys, viewer_keys) = (channel_keys(&host), channel_keys(&viewer));
        let (near, far) = pipes();
        let (accepted, connected) = tokio::join!(
            Channel::accept(far, &host_keys),
            Channel::connect(near, &viewer_keys, "host-device", other.signing_public()),
        );
        assert!(matches!(connected, Err(Error::Trust(_))));
        drop(accepted);
    }

    #[tokio::test]
    async fn a_changed_message_ends_the_channel() {
        let (host, viewer) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let (mut near, mut far) = pair(&host, &viewer).await;
        near.transport.change = Some(|bytes| {
            let last = bytes.len() - 1;
            bytes[last] ^= 1;
        });
        near.send(&Frame::Refresh).await.unwrap();
        assert!(far.receive().await.is_err());
    }
}
