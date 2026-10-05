use std::{
    io::{Read as _, Write as _},
    sync::Arc,
    time::Duration,
};

use futures_util::{SinkExt, StreamExt as _};
use rustls::{
    ClientConfig, ClientConnection, Connection, DigitallySignedStruct, DistinguishedName,
    ServerConfig, ServerConnection, SignatureScheme,
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
        AlwaysResolvesServerRawPublicKeys,
        danger::{ClientCertVerified, ClientCertVerifier},
    },
    sign::CertifiedKey,
    version::TLS13,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
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

impl Transport for TcpStream {
    async fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        tokio::time::timeout(SEND, self.write_all(&bytes))
            .await
            .map_err(|_| Error::Closed)?
            .map_err(|_| Error::Closed)
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        let mut bytes = vec![0; MESSAGE_BYTES];
        let read = self.read(&mut bytes).await.map_err(|_| Error::Closed)?;
        bytes.truncate(read);
        Ok((read > 0).then_some(bytes))
    }
}

pub(super) enum Link {
    Relay(Box<Socket>),
    Direct(TcpStream),
}

impl Transport for Link {
    async fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        match self {
            Self::Relay(socket) => Transport::send(socket.as_mut(), bytes).await,
            Self::Direct(stream) => Transport::send(stream, bytes).await,
        }
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        match self {
            Self::Relay(socket) => Transport::receive(socket.as_mut()).await,
            Self::Direct(stream) => Transport::receive(stream).await,
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

impl<T: Transport> Channel<T> {
    pub(super) async fn connect(
        transport: T,
        signing_pkcs8: &[u8],
        host_key: &[u8],
    ) -> Result<Self> {
        let provider = provider();
        let own = certified(&provider, signing_pkcs8)?;
        let expected = public_key_info(&own, host_key)?;
        let mut config = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[&TLS13])
            .map_err(tls)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(DeviceKey {
                expected: Some(expected),
                algorithms: provider.signature_verification_algorithms,
            }))
            .with_client_cert_resolver(Arc::new(AlwaysResolvesClientRawPublicKeys::new(own)));
        config.resumption = Resumption::disabled();
        config.enable_sni = false;
        let name = ServerName::try_from("kodosi.invalid").map_err(|_| invalid("Invalid name."))?;
        let tls = ClientConnection::new(Arc::new(config), name).map_err(tls)?;
        Self::establish(transport, Connection::Client(tls)).await
    }

    pub(super) async fn accept(transport: T, signing_pkcs8: &[u8]) -> Result<Self> {
        let provider = provider();
        let own = certified(&provider, signing_pkcs8)?;
        let mut config = ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[&TLS13])
            .map_err(tls)?
            .with_client_cert_verifier(Arc::new(DeviceKey {
                expected: None,
                algorithms: provider.signature_verification_algorithms,
            }))
            .with_cert_resolver(Arc::new(AlwaysResolvesServerRawPublicKeys::new(own)));
        config.send_tls13_tickets = 0;
        let tls = ServerConnection::new(Arc::new(config)).map_err(tls)?;
        Self::establish(transport, Connection::Server(tls)).await
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
        while self.tls.is_handshaking() {
            self.flush().await?;
            if !self.tls.is_handshaking() {
                break;
            }
            let message = self.transport.receive().await?.ok_or(Error::Closed)?;
            self.absorb(&message)?;
        }
        self.flush().await
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

fn public_key_info(own: &CertifiedKey, key: &[u8]) -> Result<Vec<u8>> {
    let own = own
        .cert
        .first()
        .map(AsRef::as_ref)
        .filter(|info| info.len() > DEVICE_KEY_BYTES)
        .ok_or_else(|| invalid("Stored device signing key is invalid."))?;
    if key.len() != DEVICE_KEY_BYTES {
        return Err(Error::Trust("The hosting device key is not valid.".into()));
    }
    let mut info = own[..own.len() - DEVICE_KEY_BYTES].to_vec();
    info.extend_from_slice(key);
    Ok(info)
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
    expected: Option<Vec<u8>>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl DeviceKey {
    fn check(&self, presented: &CertificateDer<'_>) -> std::result::Result<(), rustls::Error> {
        match &self.expected {
            Some(expected) if presented.as_ref() != expected.as_slice() => Err(
                rustls::Error::General("The hosting device key is not the approved key.".into()),
            ),
            _ => Ok(()),
        }
    }

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
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        self.check(end_entity)
            .map(|()| ServerCertVerified::assertion())
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
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        self.check(end_entity)
            .map(|()| ClientCertVerified::assertion())
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

    pub(in crate::network::terminal_connections) async fn pair(
        host: &DeviceKeys,
        viewer: &DeviceKeys,
    ) -> (Channel<Pipe>, Channel<Pipe>) {
        let (near, far) = pipes();
        let (accepted, connected) = tokio::join!(
            Channel::accept(far, host.signing_pkcs8()),
            Channel::connect(near, viewer.signing_pkcs8(), host.signing_public()),
        );
        (connected.unwrap(), accepted.unwrap())
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
        let (near, far) = pipes();
        let (accepted, connected) = tokio::join!(
            Channel::accept(far, host.signing_pkcs8()),
            Channel::connect(near, viewer.signing_pkcs8(), other.signing_public()),
        );
        assert!(matches!(connected, Err(Error::Trust(_))));
        assert!(accepted.is_err());
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
