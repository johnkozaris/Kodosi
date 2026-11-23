use std::{
    io,
    path::{Path, PathBuf},
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

pub struct Endpoint(imp::Endpoint);

impl Endpoint {
    pub fn new(namespace: &str, role: &str) -> io::Result<Self> {
        checked(namespace)?;
        checked(role)?;
        imp::Endpoint::new(namespace, role).map(Self)
    }

    /// An endpoint at a socket file in the user's private folder of local addresses. Some
    /// programs reach a local endpoint only by the path of a socket file, also on Windows, where
    /// the other endpoints are named pipes.
    pub fn socket_file(namespace: &str, role: &str) -> io::Result<Self> {
        checked(namespace)?;
        checked(role)?;
        imp::Endpoint::socket_file(namespace, role).map(Self)
    }

    /// Where a program reaches the endpoint: the path of its socket file, or the name of its pipe.
    pub fn address(&self) -> &Path {
        self.0.address()
    }
}

pub fn private_dir(namespace: &str) -> io::Result<PathBuf> {
    checked(namespace)?;
    imp::private_dir(namespace)
}

fn checked(part: &str) -> io::Result<()> {
    if part.is_empty()
        || part.len() > 64
        || !part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(crate::fs::invalid("invalid local endpoint name"));
    }
    Ok(())
}

pub struct Listener(imp::Listener);

impl Listener {
    pub async fn bind(endpoint: &Endpoint) -> io::Result<Self> {
        imp::Listener::bind(&endpoint.0).await.map(Self)
    }

    /// Cancel-safe.
    pub async fn accept(&mut self) -> io::Result<Stream> {
        self.0.accept().await.map(Stream)
    }
}

pub async fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    imp::connect(&endpoint.0).await.map(Stream)
}

/// Connects to a socket file that another program of this user serves, such as a file that it
/// names by a link.
pub async fn connect_file(path: &Path) -> io::Result<Stream> {
    imp::connect_file(path).await.map(Stream)
}

pub struct Stream(imp::Stream);

pub type ReadHalf = tokio::io::ReadHalf<Stream>;
pub type WriteHalf = tokio::io::WriteHalf<Stream>;

impl Stream {
    pub fn into_split(self) -> (ReadHalf, WriteHalf) {
        tokio::io::split(self)
    }

    pub fn peer_pid(&self) -> Option<u32> {
        imp::peer_pid(&self.0)
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
