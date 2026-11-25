#![allow(unsafe_code)]

use std::{
    ffi::c_void,
    fs, io,
    os::windows::io::{AsRawHandle as _, AsRawSocket as _},
    path::{Path, PathBuf},
    pin::Pin,
    ptr,
    task::{Context, Poll},
    time::Duration,
};

use socket2::{Domain, SockAddr, SockRef, Socket, Type};
use tokio::{
    io::{AsyncRead, AsyncWrite, Interest, ReadBuf},
    net::{
        TcpStream,
        windows::named_pipe::{
            ClientOptions, NamedPipeClient, NamedPipeServer, PipeMode, ServerOptions,
        },
    },
};
use windows_sys::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY},
    Networking::WinSock::{SIO_AF_UNIX_GETPEERPID, SOCKET_ERROR, WSAIoctl},
    Security::Authorization::SE_KERNEL_OBJECT,
    System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
};

use crate::win::{self, Descriptor};

const BACKLOG: i32 = 128;

pub(super) enum Endpoint {
    Pipe(String),
    File { directory: PathBuf, path: PathBuf },
}

impl Endpoint {
    pub(super) fn new(namespace: &str, role: &str) -> io::Result<Self> {
        Ok(Self::Pipe(format!(
            r"\\.\pipe\kodosi-{}-{namespace}-{role}",
            win::user_sid()?
        )))
    }

    pub(super) fn socket_file(namespace: &str, role: &str) -> io::Result<Self> {
        let directory = private_dir(namespace)?;
        let path = directory.join(format!("{role}.sock"));
        // Reject paths that exceed the AF_UNIX socket address length limit up front.
        SockAddr::unix(&path)?;
        Ok(Self::File { directory, path })
    }

    pub(super) fn address(&self) -> &Path {
        match self {
            Self::Pipe(name) => Path::new(name),
            Self::File { path, .. } => path,
        }
    }
}

// Windows `temp_dir` is per-user and inaccessible to other non-admin users.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the Unix form reads the system's folder of temporary files, and both forms share one signature"
)]
pub(super) fn private_dir(namespace: &str) -> io::Result<PathBuf> {
    Ok(std::env::temp_dir().join(format!("kodosi-{namespace}")))
}

pub(super) enum Listener {
    Pipe {
        name: String,
        descriptor: Descriptor,
        next: NamedPipeServer,
    },
    File {
        socket: TcpStream,
        directory: PathBuf,
        path: PathBuf,
    },
}

impl Listener {
    pub(super) async fn bind(endpoint: &Endpoint) -> io::Result<Self> {
        match endpoint {
            Endpoint::Pipe(name) => bind_pipe(name),
            Endpoint::File { directory, path } => bind_file(directory, path).await,
        }
    }

    pub(super) async fn accept(&mut self) -> io::Result<Stream> {
        match self {
            Self::Pipe {
                name,
                descriptor,
                next,
            } => {
                next.connect().await?;
                let following = create(name, descriptor, false)?;
                Ok(Stream::Server(std::mem::replace(next, following)))
            }
            Self::File { socket, .. } => loop {
                socket.readable().await?;
                match socket.try_io(Interest::READABLE, || SockRef::from(&*socket).accept()) {
                    Ok((accepted, _)) => return ready(accepted).map(Stream::File),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error),
                }
            },
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Self::File {
            directory, path, ..
        } = self
        {
            drop(fs::remove_file(&*path));
            drop(fs::remove_dir(&*directory));
        }
    }
}

fn bind_pipe(name: &str) -> io::Result<Listener> {
    let descriptor = Descriptor::private("GA", false, false)?;
    let next = create(name, &descriptor, true).map_err(|error| {
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) {
            io::Error::new(
                io::ErrorKind::AddrInUse,
                "another process owns this local endpoint",
            )
        } else {
            error
        }
    })?;
    Ok(Listener::Pipe {
        name: name.to_owned(),
        descriptor,
        next,
    })
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

async fn bind_file(directory: &Path, path: &Path) -> io::Result<Listener> {
    crate::fs::create_user_only_dir(directory)?;
    if fs::symlink_metadata(path).is_ok() {
        if connect_file(path).await.is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another process owns this local endpoint",
            ));
        }
        // Remove stale socket files left behind by crashed listeners.
        fs::remove_file(path)?;
    }
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    socket.bind(&SockAddr::unix(path)?)?;
    socket.listen(BACKLOG)?;
    Ok(Listener::File {
        socket: ready(socket)?,
        directory: directory.to_owned(),
        path: path.to_owned(),
    })
}

// Tokio lacks AF_UNIX support on Windows, but Windows AF_UNIX sockets support
// the same readiness and I/O operations as `TcpStream`.
fn ready(socket: Socket) -> io::Result<TcpStream> {
    socket.set_nonblocking(true)?;
    TcpStream::from_std(socket.into())
}

pub(super) async fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    match endpoint {
        Endpoint::Pipe(name) => connect_pipe(name).await,
        Endpoint::File { path, .. } => {
            if fs::symlink_metadata(path).is_err() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "nothing listens at the local endpoint",
                ));
            }
            connect_file(path).await
        }
    }
}

async fn connect_pipe(name: &str) -> io::Result<Stream> {
    let client = loop {
        match ClientOptions::new().open(name) {
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

pub(super) async fn connect_file(path: &Path) -> io::Result<Stream> {
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    match socket.connect(&SockAddr::unix(path)?) {
        Err(error) if error.kind() != io::ErrorKind::WouldBlock => return Err(error),
        _ => {}
    }
    let stream = TcpStream::from_std(socket.into())?;
    stream.writable().await?;
    if let Some(error) = stream.take_error()? {
        return Err(error);
    }
    Ok(Stream::File(stream))
}

pub(super) enum Stream {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
    File(TcpStream),
}

pub(super) fn peer_pid(stream: &Stream) -> Option<u32> {
    let mut pid = 0;
    let found = match stream {
        Stream::Server(pipe) => unsafe {
            GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid) != 0
        },
        Stream::Client(pipe) => unsafe {
            GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut pid) != 0
        },
        Stream::File(socket) => {
            let mut returned = 0;
            let socket = usize::try_from(socket.as_raw_socket()).ok()?;
            unsafe {
                WSAIoctl(
                    socket,
                    SIO_AF_UNIX_GETPEERPID,
                    ptr::null(),
                    0,
                    (&raw mut pid).cast(),
                    u32::try_from(size_of::<u32>()).ok()?,
                    &raw mut returned,
                    ptr::null_mut(),
                    None,
                ) != SOCKET_ERROR
            }
        }
    };
    (found && pid != 0).then_some(pid)
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
            Self::File(socket) => Pin::new(socket).poll_read(context, buffer),
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
            Self::File(socket) => Pin::new(socket).poll_write(context, buffer),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(pipe) => Pin::new(pipe).poll_flush(context),
            Self::Client(pipe) => Pin::new(pipe).poll_flush(context),
            Self::File(socket) => Pin::new(socket).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(pipe) => Pin::new(pipe).poll_shutdown(context),
            Self::Client(pipe) => Pin::new(pipe).poll_shutdown(context),
            Self::File(socket) => Pin::new(socket).poll_shutdown(context),
        }
    }
}
