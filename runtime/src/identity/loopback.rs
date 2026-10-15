//! Takes the answer of a sign-in in the system browser on a loopback address (RFC 8252).

use std::{net::Ipv4Addr, time::Duration};

use reqwest::Url;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    task::JoinSet,
    time::timeout,
};

use super::oidc::BrowserLogin;
use crate::network::Result;

const MAX_REQUEST: usize = 8 * 1024;
const MAX_WAITING: usize = 16;
const READ_TIME: Duration = Duration::from_secs(10);
const WRITE_TIME: Duration = Duration::from_secs(5);

pub(crate) struct Loopback {
    listener: TcpListener,
    redirect_uri: String,
}

impl Loopback {
    pub(crate) async fn bind() -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let redirect_uri = format!(
            "http://127.0.0.1:{}/callback",
            listener.local_addr()?.port()
        );
        Ok(Self {
            listener,
            redirect_uri,
        })
    }

    pub(crate) fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Waits for the browser's answer to `login`. Other requests get a short page, so a
    /// browser that opens spare connections or asks for an icon does not hold the answer.
    pub(crate) async fn answer(&self, login: &BrowserLogin) -> Result<(Url, TcpStream)> {
        let mut requests = JoinSet::new();
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted?;
                    if requests.len() < MAX_WAITING {
                        requests.spawn(timeout(READ_TIME, target(stream)));
                    }
                }
                Some(read) = requests.join_next() => {
                    let Ok(Ok(Ok(Some((target, stream))))) = read else { continue };
                    match Url::parse(&self.redirect_uri).and_then(|base| base.join(&target)) {
                        Ok(answer) if login.owns(&answer) => return Ok((answer, stream)),
                        _ => reply(stream, "404 Not Found", NOT_HERE).await,
                    }
                }
            }
        }
    }
}

/// Tells the person in the browser how the sign-in ended.
pub(crate) async fn finish(stream: TcpStream, outcome: std::result::Result<(), String>) {
    let page = match outcome {
        Ok(()) => page(
            "You are signed in to Kodosi",
            "Close this tab and go back to Kodosi.",
        ),
        Err(message) => page(
            "Kodosi could not sign you in",
            &format!("{} Go back to Kodosi and try again.", escape(&message)),
        ),
    };
    reply(stream, "200 OK", &page).await;
}

/// The request target of one GET request, or nothing for any other request.
async fn target(mut stream: TcpStream) -> std::io::Result<Option<(String, TcpStream)>> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|end| end == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await?;
        if read == 0 || request.len() + read > MAX_REQUEST {
            return Ok(None);
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let line = request
        .split(|&byte| byte == b'\r')
        .next()
        .unwrap_or_default();
    let mut words = std::str::from_utf8(line).unwrap_or_default().split(' ');
    match (words.next(), words.next(), words.next()) {
        (Some("GET"), Some(target), Some(version))
            if target.starts_with('/') && version.starts_with("HTTP/1.") =>
        {
            Ok(Some((target.to_owned(), stream)))
        }
        _ => Ok(None),
    }
}

async fn reply(mut stream: TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\n\
         Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
         X-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _written = timeout(WRITE_TIME, async {
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body.as_bytes()).await?;
        stream.shutdown().await
    })
    .await;
}

const NOT_HERE: &str = "<!doctype html><title>Kodosi</title>";

fn page(title: &str, text: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width\"><title>Kodosi</title>\
         <style>:root{{color-scheme:light dark}}body{{margin:0;min-height:90vh;display:grid;\
         place-items:center;font:16px/1.5 -apple-system,system-ui,sans-serif}}\
         main{{max-width:28em;padding:24px;text-align:center}}h1{{font-size:20px;margin:0 0 6px}}\
         p{{margin:0;opacity:.7}}</style><main><h1>{title}</h1><p>{text}</p></main></html>"
    )
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            _ => character.to_string(),
        })
        .collect()
}
