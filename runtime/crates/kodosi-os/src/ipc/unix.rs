use std::{
    fs::{self, Permissions},
    io,
    os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
};

use rustix::{io::Errno, process::geteuid};
use tokio::net::{UnixListener, UnixStream};

pub(super) type Stream = UnixStream;

pub(super) struct Endpoint {
    directory: PathBuf,
    path: PathBuf,
}

impl Endpoint {
    pub(super) fn new(namespace: &str, role: &str) -> io::Result<Self> {
        let directory = crate::fs::canonicalize(Path::new("/tmp"))?
            .join(format!("kodosi-{}-{namespace}", geteuid().as_raw()));
        let path = directory.join(format!("{role}.sock"));
        Ok(Self { directory, path })
    }
}

pub(super) struct Listener {
    inner: UnixListener,
    directory: PathBuf,
    path: PathBuf,
}

impl Listener {
    pub(super) async fn bind(endpoint: &Endpoint) -> io::Result<Self> {
        match Self::bind_once(endpoint).await {
            Err(error) if directory_removed(&error) => Self::bind_once(endpoint).await,
            result => result,
        }
    }

    async fn bind_once(endpoint: &Endpoint) -> io::Result<Self> {
        crate::fs::create_private_dir(&endpoint.directory)?;
        if existing_socket(&endpoint.path)? {
            match UnixStream::connect(&endpoint.path).await {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "another process owns this local endpoint",
                    ));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) =>
                {
                    fs::remove_file(&endpoint.path)?;
                }
                Err(error) => return Err(error),
            }
        }
        let inner = UnixListener::bind(&endpoint.path)?;
        fs::set_permissions(&endpoint.path, Permissions::from_mode(0o600))?;
        Ok(Self {
            inner,
            directory: endpoint.directory.clone(),
            path: endpoint.path.clone(),
        })
    }

    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "the Windows listener replaces its waiting pipe instance"
    )]
    pub(super) async fn accept(&mut self) -> io::Result<Stream> {
        loop {
            let (stream, _) = self.inner.accept().await?;
            if stream
                .peer_cred()
                .is_ok_and(|peer| peer.uid() == geteuid().as_raw())
            {
                return Ok(stream);
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if existing_socket(&self.path).unwrap_or(false) {
            drop(fs::remove_file(&self.path));
        }
        drop(fs::remove_dir(&self.directory));
    }
}

pub(super) async fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    if !existing_socket(&endpoint.path)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "nothing listens at the local endpoint",
        ));
    }
    let stream = UnixStream::connect(&endpoint.path).await?;
    if stream.peer_cred()?.uid() != geteuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the local endpoint belongs to another user",
        ));
    }
    Ok(stream)
}

pub(super) fn peer_pid(stream: &Stream) -> Option<u32> {
    let pid = stream.peer_cred().ok()?.pid()?;
    u32::try_from(pid).ok()
}

fn existing_socket(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.uid() != geteuid().as_raw()
                || metadata.mode() & 0o077 != 0
                || !metadata.file_type().is_socket()
            {
                return Err(crate::fs::invalid(format!(
                    "unsafe local endpoint: {}",
                    path.display()
                )));
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn directory_removed(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound || Errno::from_io_error(error) == Some(Errno::INVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn endpoints_refuse_foreign_files_and_links() {
        let endpoint = Endpoint::new(&format!("shape-{}", std::process::id()), "host").unwrap();
        crate::fs::create_private_dir(&endpoint.directory).unwrap();
        fs::write(&endpoint.path, b"not a socket").unwrap();
        fs::set_permissions(&endpoint.path, Permissions::from_mode(0o600)).unwrap();
        assert!(connect(&endpoint).await.is_err());
        assert!(Listener::bind(&endpoint).await.is_err());
        fs::remove_file(&endpoint.path).unwrap();
        std::os::unix::fs::symlink("/dev/null", &endpoint.path).unwrap();
        assert!(connect(&endpoint).await.is_err());
        fs::remove_file(&endpoint.path).unwrap();
        fs::remove_dir(&endpoint.directory).unwrap();
    }

    #[tokio::test]
    async fn the_last_listener_removes_the_directory() {
        let namespace = format!("last-{}", std::process::id());
        let host = Endpoint::new(&namespace, "host").unwrap();
        let app = Endpoint::new(&namespace, "app").unwrap();
        let app_listener = Listener::bind(&app).await.unwrap();
        drop(Listener::bind(&host).await.unwrap());
        assert!(connect(&app).await.is_ok());
        drop(app_listener);
        assert!(!host.directory.exists());
        drop(Listener::bind(&host).await.unwrap());
        assert!(!host.directory.exists());
    }
}
