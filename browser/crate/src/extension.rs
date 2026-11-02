use std::{fmt::Write as _, io, path::Path, time::Duration};

use bytes::Bytes;
use futures_util::{SinkExt as _, StreamExt as _};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::tungstenite::{
    Message,
    handshake::server::{Callback, ErrorResponse, Request, Response},
    http::StatusCode,
};

use crate::{Error, message::MenuItem};

const EXTENSION_FILES: &[(&str, &str)] = &[
    (
        "manifest.json",
        include_str!("../../extension/manifest.json"),
    ),
    (
        "offscreen.html",
        include_str!("../../extension/offscreen.html"),
    ),
    ("offscreen.js", include_str!("../../extension/offscreen.js")),
    ("page.js", include_str!("../../extension/page.js")),
    ("worker.js", include_str!("../../extension/worker.js")),
];
const CONNECT_LIMIT: Duration = Duration::from_secs(30);
const HEADER: usize = 8 + 1 + 8;

/// The id that the key in the extension's manifest gives it.
pub(crate) const ID: &str = "iohogdknhgghkkodmgaljlgbnagghpgf";

/// A command for the extension. A stream is the id of the window whose picture it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ToExtension {
    Capture {
        stream: i64,
        tab: i64,
        width: u32,
        height: u32,
        bitrate: u32,
    },
    Stop {
        stream: i64,
    },
    Keyframe {
        stream: i64,
    },
    Bitrate {
        stream: i64,
        bitrate: u32,
    },
    Activate {
        tab: i64,
    },
    NewTab {
        window: i64,
    },
    CloseTab {
        tab: i64,
    },
    CloseWindow {
        window: i64,
    },
    Pick {
        tab: i64,
        frame: i64,
        menu: u32,
        choice: Option<u32>,
    },
    Report,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ReportedWindow {
    pub id: i64,
    pub popup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ReportedTab {
    pub id: i64,
    pub window: i64,
    pub opener: Option<i64>,
    pub target: Option<String>,
    pub title: String,
    pub address: String,
    pub active: bool,
    pub loading: bool,
}

/// What the extension tells the host.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum FromExtension {
    Tabs {
        windows: Vec<ReportedWindow>,
        tabs: Vec<ReportedTab>,
    },
    Shape {
        tab: i64,
        shape: String,
    },
    Menu {
        tab: i64,
        frame: i64,
        menu: u32,
        items: Vec<MenuItem>,
    },
    Context {
        tab: i64,
        selection: String,
        link: Option<String>,
    },
    Copied {
        tab: i64,
        text: String,
    },
    Failed {
        stream: i64,
        problem: String,
    },
    #[serde(skip)]
    Frame(EncodedFrame),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EncodedFrame {
    pub stream: i64,
    pub key: bool,
    pub timestamp: u64,
    pub data: Bytes,
}

fn encoded_frame(message: &Bytes) -> Option<EncodedFrame> {
    let header = message.get(..HEADER)?;
    let stream = i64::from_be_bytes(header[..8].try_into().ok()?);
    let timestamp = u64::from_be_bytes(header[9..].try_into().ok()?);
    Some(EncodedFrame {
        stream,
        key: header[8] != 0,
        timestamp,
        data: message.slice(HEADER..),
    })
}

/// The socket on which the host waits for its extension.
pub(crate) struct Socket {
    listener: TcpListener,
    pub port: u16,
    pub token: String,
}

impl Socket {
    pub(crate) async fn open() -> Result<Self, Error> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            port,
            token: token()?,
        })
    }

    /// Takes the one connection that shows the token, and then stops listening, so the token
    /// opens nothing a second time.
    pub(crate) async fn accept(
        self,
        commands: mpsc::Receiver<ToExtension>,
        reports: mpsc::Sender<FromExtension>,
    ) -> Result<(), Error> {
        let path = format!("/{}", self.token);
        let socket = tokio::time::timeout(CONNECT_LIMIT, async {
            loop {
                let (stream, _) = self.listener.accept().await?;
                let path = path.clone();
                let accepted =
                    tokio_tungstenite::accept_hdr_async(stream, Admission { path }).await;
                if let Ok(socket) = accepted {
                    return Ok::<_, Error>(socket);
                }
            }
        })
        .await
        .map_err(|_| Error::Engine("the extension did not connect".to_owned()))??;
        drop(self.listener);
        carry(socket, commands, reports).await;
        Ok(())
    }
}

struct Admission {
    path: String,
}

impl Callback for Admission {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let origin = format!("chrome-extension://{ID}");
        let from_extension = request
            .headers()
            .get("origin")
            .is_some_and(|value| value == origin.as_str());
        if from_extension && request.uri().path() == self.path {
            Ok(response)
        } else {
            let mut refused = ErrorResponse::new(None);
            *refused.status_mut() = StatusCode::FORBIDDEN;
            Err(refused)
        }
    }
}

async fn carry<S>(
    socket: S,
    mut commands: mpsc::Receiver<ToExtension>,
    reports: mpsc::Sender<FromExtension>,
) where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures_util::Sink<Message>
        + Unpin,
{
    let (mut sink, mut source) = socket.split();
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { return };
                let Ok(text) = serde_json::to_string(&command) else {
                    continue;
                };
                if sink.send(Message::text(text)).await.is_err() {
                    return;
                }
            }
            message = source.next() => {
                let report = match message {
                    Some(Ok(Message::Binary(body))) => encoded_frame(&body).map(FromExtension::Frame),
                    Some(Ok(Message::Text(text))) => serde_json::from_str(&text).ok(),
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    Some(Ok(_)) => None,
                };
                if let Some(report) = report
                    && reports.send(report).await.is_err()
                {
                    return;
                }
            }
        }
    }
}

/// Copies the extension into the profile with the address and the token of this start.
pub(crate) async fn prepare(into: &Path, socket: &Socket) -> Result<(), Error> {
    tokio::fs::create_dir_all(into).await?;
    for (name, content) in EXTENSION_FILES {
        tokio::fs::write(into.join(name), content).await?;
    }
    let connection = serde_json::json!({ "port": socket.port, "token": socket.token });
    tokio::fs::write(into.join("connection.json"), connection.to_string()).await?;
    Ok(())
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
    use base64::{Engine as _, prelude::BASE64_STANDARD};
    use sha2::{Digest as _, Sha256};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    use super::*;

    fn frame_message(stream: i64, key: u8, timestamp: u64, data: &[u8]) -> Bytes {
        let mut message = stream.to_be_bytes().to_vec();
        message.push(key);
        message.extend(timestamp.to_be_bytes());
        message.extend(data);
        message.into()
    }

    #[test]
    fn a_frame_names_its_stream() {
        let frame = encoded_frame(&frame_message(9, 1, 33, &[1, 2])).unwrap();
        assert_eq!(frame.stream, 9);
        assert!(frame.key);
        assert_eq!(frame.timestamp, 33);
        assert_eq!(&frame.data[..], [1, 2]);
    }

    #[test]
    fn a_short_message_is_no_frame() {
        assert_eq!(encoded_frame(&Bytes::from_static(&[0, 1])), None);
    }

    #[test]
    fn the_id_comes_from_the_key_in_the_manifest() {
        let manifest = include_str!("../../extension/manifest.json");
        let manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
        let key = BASE64_STANDARD
            .decode(manifest["key"].as_str().unwrap())
            .unwrap();
        let id: String = Sha256::digest(key)[..16]
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 15])
            .map(|digit| char::from(b'a' + digit))
            .collect();
        assert_eq!(id, ID);
    }

    #[tokio::test]
    async fn only_the_extension_with_the_token_connects() {
        let socket = Socket::open().await.unwrap();
        let port = socket.port;
        let token = socket.token.clone();
        let (_commands, waiting) = mpsc::channel(1);
        let (reports, mut incoming) = mpsc::channel(1);
        let accepted = tokio::spawn(socket.accept(waiting, reports));
        let address = format!("ws://127.0.0.1:{port}/{token}");
        let origin: tokio_tungstenite::tungstenite::http::HeaderValue =
            format!("chrome-extension://{ID}").parse().unwrap();

        let mut wrong_token = format!("ws://127.0.0.1:{port}/wrong")
            .into_client_request()
            .unwrap();
        wrong_token.headers_mut().insert("origin", origin.clone());
        assert!(tokio_tungstenite::connect_async(wrong_token).await.is_err());

        let no_origin = address.clone().into_client_request().unwrap();
        assert!(tokio_tungstenite::connect_async(no_origin).await.is_err());

        let mut request = address.clone().into_client_request().unwrap();
        request.headers_mut().insert("origin", origin.clone());
        let (mut extension, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        extension
            .send(Message::text(r#"{"type":"copied","tab":1,"text":"word"}"#))
            .await
            .unwrap();
        assert!(matches!(
            incoming.recv().await,
            Some(FromExtension::Copied { tab: 1, .. })
        ));

        let mut second = address.into_client_request().unwrap();
        second.headers_mut().insert("origin", origin);
        assert!(tokio_tungstenite::connect_async(second).await.is_err());

        drop(extension);
        assert!(accepted.await.unwrap().is_ok());
    }
}
