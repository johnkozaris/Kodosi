//! The requests of an agent's link: HTTP/1.1 with a JSON body of a known length.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

const MOST_HEAD: usize = 16 * 1024;
const MOST_BODY: usize = 8 * 1024 * 1024;
const MOST_HEADERS: usize = 32;

pub(super) struct Request {
    pub(super) path: String,
    pub(super) body: Vec<u8>,
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

/// The next request, or none when the other side closed the connection between two requests.
/// `kept` holds what arrived after the request before.
pub(super) async fn read<S: AsyncRead + Unpin>(
    stream: &mut S,
    kept: &mut Vec<u8>,
) -> io::Result<Option<Request>> {
    let (head, path, length) = loop {
        let mut headers = [httparse::EMPTY_HEADER; MOST_HEADERS];
        let mut request = httparse::Request::new(&mut headers);
        match request.parse(kept) {
            Ok(httparse::Status::Complete(head)) => {
                let mut length = 0;
                for header in request.headers.iter() {
                    if header.name.eq_ignore_ascii_case("content-length") {
                        length = std::str::from_utf8(header.value)
                            .ok()
                            .and_then(|value| value.trim().parse::<usize>().ok())
                            .ok_or_else(|| invalid("the length of the body is not a number"))?;
                    } else if header.name.eq_ignore_ascii_case("transfer-encoding") {
                        return Err(invalid("the body has no known length"));
                    }
                }
                if length > MOST_BODY {
                    return Err(invalid("the body is too large"));
                }
                break (head, request.path.unwrap_or_default().to_owned(), length);
            }
            Ok(httparse::Status::Partial) if kept.len() > MOST_HEAD => {
                return Err(invalid("the head of the request is too large"));
            }
            Ok(httparse::Status::Partial) => {
                if stream.read_buf(kept).await? == 0 {
                    return if kept.is_empty() {
                        Ok(None)
                    } else {
                        Err(io::ErrorKind::UnexpectedEof.into())
                    };
                }
            }
            Err(_) => return Err(invalid("the request is not HTTP")),
        }
    };
    while kept.len() < head + length {
        if stream.read_buf(kept).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }
    let body = kept[head..head + length].to_vec();
    kept.drain(..head + length);
    Ok(Some(Request { path, body }))
}

pub(super) async fn answer<S: AsyncWrite + Unpin>(
    stream: &mut S,
    status: u16,
    body: &[u8],
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        409 => "Conflict",
        410 => "Gone",
        _ => "Bad Request",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}
