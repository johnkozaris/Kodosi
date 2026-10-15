#![allow(unsafe_code)]

use std::{
    ffi::c_void,
    io,
    os::windows::io::AsRawHandle as _,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, PipeMode, ServerOptions,
    },
};
use windows_sys::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY},
    Security::Authorization::SE_KERNEL_OBJECT,
    System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
};

use crate::win::{self, Descriptor};

pub(super) struct Endpoint {
    name: String,
}

impl Endpoint {
    pub(super) fn new(namespace: &str, role: &str) -> io::Result<Self> {
        Ok(Self {
            name: format!(r"\\.\pipe\kodosi-{}-{namespace}-{role}", win::user_sid()?),
        })
    }
}

pub(super) struct Listener {
    name: String,
    descriptor: Descriptor,
    next: NamedPipeServer,
}

impl Listener {
    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "binding is asynchronous on Unix, which probes a stale socket"
    )]
    pub(super) async fn bind(endpoint: &Endpoint) -> io::Result<Self> {
        let descriptor = Descriptor::private("GA", false, false)?;
        let next = create(&endpoint.name, &descriptor, true).map_err(|error| {
            if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) {
                io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another process owns this local endpoint",
                )
            } else {
                error
            }
        })?;
        Ok(Self {
            name: endpoint.name.clone(),
            descriptor,
            next,
        })
    }

    pub(super) async fn accept(&mut self) -> io::Result<Stream> {
        self.next.connect().await?;
        let next = create(&self.name, &self.descriptor, false)?;
        Ok(Stream::Server(std::mem::replace(&mut self.next, next)))
    }
}

fn create(name: &str, descriptor: &Descriptor, first: bool) -> io::Result<NamedPipeServer> {
    let mut attributes = descriptor.attributes();
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .pipe_mode(PipeMode::Byte);
    unsafe {
        options.create_with_security_attributes_raw(name, (&raw mut attributes).cast::<c_void>())
    }
}

pub(super) async fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    let client = loop {
        match ClientOptions::new().open(&endpoint.name) {
            Ok(client) => break client,
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY.cast_signed()) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    };
    if !win::owned_by_user(client.as_raw_handle(), SE_KERNEL_OBJECT)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the local endpoint belongs to another user",
        ));
    }
    Ok(Stream::Client(client))
}

pub(super) enum Stream {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

pub(super) fn peer_pid(stream: &Stream) -> Option<u32> {
    let mut pid = 0;
    let found = unsafe {
        match stream {
            Stream::Server(pipe) => GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid),
            Stream::Client(pipe) => GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut pid),
        }
    };
    (found != 0 && pid != 0).then_some(pid)
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(pipe) => Pin::new(pipe).poll_read(context, buffer),
            Self::Client(pipe) => Pin::new(pipe).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Server(pipe) => Pin::new(pipe).poll_write(context, buffer),
            Self::Client(pipe) => Pin::new(pipe).poll_write(context, buffer),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(pipe) => Pin::new(pipe).poll_flush(context),
            Self::Client(pipe) => Pin::new(pipe).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(pipe) => Pin::new(pipe).poll_shutdown(context),
            Self::Client(pipe) => Pin::new(pipe).poll_shutdown(context),
        }
    }
}
