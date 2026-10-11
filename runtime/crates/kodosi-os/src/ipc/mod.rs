use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// A local endpoint that only the current user can reach, named by a
/// `namespace`, such as a hash of the data folder, and a `role`.
pub struct Endpoint(imp::Endpoint);

impl Endpoint {
    pub fn new(namespace: &str, role: &str) -> io::Result<Self> {
        for part in [namespace, role] {
            if part.is_empty()
                || part.len() > 64
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return Err(crate::fs::invalid("invalid local endpoint name"));
            }
        }
        imp::Endpoint::new(namespace, role).map(Self)
    }
}

/// Accepts connections from processes that the current user runs.
pub struct Listener(imp::Listener);

impl Listener {
    /// Fails with `AddrInUse` while another live listener owns the endpoint.
    pub async fn bind(endpoint: &Endpoint) -> io::Result<Self> {
        imp::Listener::bind(&endpoint.0).await.map(Self)
    }

    /// Waits for the next client. Cancel-safe.
    pub async fn accept(&mut self) -> io::Result<Stream> {
        self.0.accept().await.map(Stream)
    }
}

/// Connects to the listener at `endpoint`. Fails with `NotFound` when nothing
/// listens and with `PermissionDenied` when another user owns the endpoint.
pub async fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    imp::connect(&endpoint.0).await.map(Stream)
}

pub struct Stream(imp::Stream);

pub type ReadHalf = tokio::io::ReadHalf<Stream>;
pub type WriteHalf = tokio::io::WriteHalf<Stream>;

impl Stream {
    pub fn into_split(self) -> (ReadHalf, WriteHalf) {
        tokio::io::split(self)
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(context, buffer)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(context, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(context)
    }
}

#[cfg(test)]
mod tests;
