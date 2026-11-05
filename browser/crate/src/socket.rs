use std::{fmt::Write as _, io, path::Path, time::Duration};

use futures_util::{SinkExt as _, StreamExt as _};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::tungstenite::{
    Message,
    handshake::server::{Callback, ErrorResponse, Request, Response},
    http::StatusCode,
};

use crate::{
    Error,
    link::{Address, EncodedFrame, FromEngine, ToEngine},
};

const CONNECT_LIMIT: Duration = Duration::from_secs(30);
const HANDSHAKE_LIMIT: Duration = Duration::from_secs(5);

/// What the engine says: a report, or a picture of a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Report {
    Said(FromEngine),
    Frame(EncodedFrame),
}

/// The socket on which the host waits for its engine.
pub(crate) struct Socket {
    listener: TcpListener,
    pub address: Address,
}

impl Socket {
    pub(crate) async fn open() -> Result<Self, Error> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            address: Address {
                port,
                token: token()?,
            },
        })
    }

    /// Writes where the engine finds this socket, readable only by this user.
    pub(crate) async fn write_address(&self, file: &Path) -> Result<(), Error> {
        let text = serde_json::to_vec(&self.address).map_err(io::Error::other)?;
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut written = options.open(file).await?;
        tokio::io::AsyncWriteExt::write_all(&mut written, &text).await?;
        tokio::io::AsyncWriteExt::flush(&mut written).await?;
        Ok(())
    }

    /// Takes the one connection that shows the token, then stops listening, so the token opens
    /// nothing a second time. The link lasts as long as the engine.
    pub(crate) async fn accept(
        self,
        mut commands: mpsc::Receiver<ToEngine>,
        reports: mpsc::Sender<Report>,
    ) -> Result<(), Error> {
        let path = self.address.path();
        let socket = tokio::time::timeout(CONNECT_LIMIT, async {
            loop {
                let (stream, _) = self.listener.accept().await?;
                let admission = Admission { path: path.clone() };
                let handshake = tokio_tungstenite::accept_hdr_async(stream, admission);
                if let Ok(Ok(socket)) = tokio::time::timeout(HANDSHAKE_LIMIT, handshake).await {
                    return Ok::<_, Error>(socket);
                }
            }
        })
        .await
        .map_err(|_| Error::Engine("the browser did not connect".to_owned()))??;
        drop(self.listener);
        let (mut sink, mut source) = socket.split();
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { return Ok(()) };
                    let Ok(text) = serde_json::to_string(&command) else {
                        continue;
                    };
                    if sink.send(Message::text(text)).await.is_err() {
                        return Ok(());
                    }
                }
                message = source.next() => {
                    let report = match message {
                        Some(Ok(Message::Binary(body))) => EncodedFrame::from_message(&body).map(Report::Frame),
                        Some(Ok(Message::Text(text))) => serde_json::from_str(&text).ok().map(Report::Said),
                        Some(Ok(Message::Close(_)) | Err(_)) | None => return Ok(()),
                        Some(Ok(_)) => None,
                    };
                    if let Some(report) = report
                        && reports.send(report).await.is_err()
                    {
                        return Ok(());
                    }
                }
            }
        }
    }
}

struct Admission {
    path: String,
}

impl Callback for Admission {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        if request.uri().path() == self.path {
            Ok(response)
        } else {
            let mut refused = ErrorResponse::new(None);
            *refused.status_mut() = StatusCode::FORBIDDEN;
            Err(refused)
        }
    }
}

fn token() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        write!(text, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    #[tokio::test]
    async fn only_the_engine_with_the_token_connects_and_only_once() {
        let socket = Socket::open().await.unwrap();
        let address = socket.address.clone();
        let (commands, waiting) = mpsc::channel(1);
        let (reports, mut incoming) = mpsc::channel(1);
        let accepted = tokio::spawn(socket.accept(waiting, reports));

        let wrong = format!("ws://127.0.0.1:{}/wrong", address.port);
        assert!(tokio_tungstenite::connect_async(wrong).await.is_err());

        let right = format!("ws://127.0.0.1:{}{}", address.port, address.path());
        let (mut engine, _) = tokio_tungstenite::connect_async(right.clone())
            .await
            .unwrap();
        engine
            .send(Message::text(r#"{"type":"copied","tab":1,"text":"word"}"#))
            .await
            .unwrap();
        assert!(matches!(
            incoming.recv().await,
            Some(Report::Said(FromEngine::Copied { tab: 1, .. }))
        ));
        let frame = EncodedFrame {
            window: 4,
            key: true,
            timestamp: 9,
            data: Bytes::from_static(b"\0\0\0\x01x"),
        };
        engine
            .send(Message::binary(frame.to_message()))
            .await
            .unwrap();
        assert_eq!(incoming.recv().await, Some(Report::Frame(frame)));

        assert!(tokio_tungstenite::connect_async(right).await.is_err());

        commands.send(ToEngine::Quit).await.unwrap();
        let said = engine.next().await.unwrap().unwrap();
        assert_eq!(said, Message::text(r#"{"type":"quit"}"#));
        drop(commands);
        assert!(accepted.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn the_address_file_is_private() {
        let folder = tempfile::tempdir().unwrap();
        let socket = Socket::open().await.unwrap();
        let file = folder.path().join("link.json");
        socket.write_address(&file).await.unwrap();
        let read: Address = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(read, socket.address);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
