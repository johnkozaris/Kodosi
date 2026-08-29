use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{AppError, Result};

const SOCKET_FILE_NAME: &str = "headless-host.sock";

pub(in crate::headless_host) fn socket_path_for_dir(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(SOCKET_FILE_NAME)
}

pub(in crate::headless_host) type LocalListener = tokio::net::UnixListener;

pub(in crate::headless_host) type LocalStream = tokio::net::UnixStream;

pub(in crate::headless_host) type LocalWriteHalf = tokio::net::unix::OwnedWriteHalf;

pub(in crate::headless_host) async fn bind_local(path: &Path) -> Result<LocalListener> {
    use crate::support::platform::fs as support_fs;

    let parent = path.parent().ok_or_else(|| AppError::Unsupported {
        reason: "socket path has no parent directory".to_owned(),
    })?;
    support_fs::ensure_dir(parent)?;
    match tokio::net::UnixStream::connect(path).await {
        Ok(_) => {
            return Err(AppError::Io(io::Error::new(
                io::ErrorKind::AddrInUse,
                "headless host socket already has a live listener",
            )));
        }
        Err(error) if is_connection_missing(&error) => {
            drop(std::fs::remove_file(path));
        }
        Err(error) => return Err(AppError::Io(error)),
    }
    tokio::net::UnixListener::bind(path).map_err(AppError::Io)
}

pub(in crate::headless_host) fn acquire_host_lock(runtime_dir: &Path) -> Result<std::fs::File> {
    use crate::support::platform::fs as support_fs;
    support_fs::ensure_dir(runtime_dir)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(runtime_dir.join("headless-host.lock"))?;
    lock.try_lock().map_err(|error| {
        AppError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("another headless host owns the runtime directory: {error}"),
        ))
    })?;
    Ok(lock)
}

pub(in crate::headless_host) fn host_lock_is_free(runtime_dir: &Path) -> bool {
    match acquire_host_lock(runtime_dir) {
        Ok(_lock) => true,
        Err(AppError::Io(ref error)) if error.kind() == io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            tracing::warn!(%error, "could not probe the headless host runtime lock");
            true
        }
    }
}

pub(in crate::headless_host) fn remove_socket_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not unlink the host socket");
        }
    }
}

pub(in crate::headless_host) async fn accept_local(
    listener: &LocalListener,
) -> io::Result<LocalStream> {
    listener.accept().await.map(|(stream, _addr)| stream)
}

pub(in crate::headless_host) async fn connect_local(path: &Path) -> io::Result<LocalStream> {
    tokio::net::UnixStream::connect(path).await
}

const CONNECT_RETRY_ATTEMPTS: u32 = 5;

const CONNECT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(40);

const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

async fn connect_local_bounded(path: &Path) -> io::Result<LocalStream> {
    match tokio::time::timeout(CONNECT_TIMEOUT, connect_local(path)).await {
        Ok(result) => result,
        Err(_elapsed) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "connecting to the headless host socket took longer than {}s",
                CONNECT_TIMEOUT.as_secs()
            ),
        )),
    }
}

pub(in crate::headless_host) async fn connect_local_retrying(
    path: &Path,
) -> io::Result<LocalStream> {
    let mut last_err: Option<io::Error> = None;
    for attempt in 0..CONNECT_RETRY_ATTEMPTS {
        match connect_local_bounded(path).await {
            Ok(stream) => return Ok(stream),
            Err(error) if is_connection_missing(&error) => {
                last_err = Some(error);
                if attempt + 1 < CONNECT_RETRY_ATTEMPTS {
                    tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_err
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "connect retries exhausted")))
}

pub(in crate::headless_host) fn is_connection_missing(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::NotFound
            | io::ErrorKind::AddrNotAvailable
    )
}

pub(in crate::headless_host) fn is_not_a_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;

    std::fs::metadata(path).is_ok_and(|meta| !meta.file_type().is_socket())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_for_dir_appends_filename() {
        let dir = Path::new("/run/user/1000/kodosi");
        let path = socket_path_for_dir(dir);
        assert_eq!(path, dir.join("headless-host.sock"));
    }

    #[test]
    fn socket_path_for_dir_segments_are_correct() {
        let dir = Path::new("/run/kodosi");
        let path = socket_path_for_dir(dir);
        let file_name = path.file_name().and_then(|n| n.to_str());
        assert_eq!(file_name, Some("headless-host.sock"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bind_and_connect_local_roundtrip() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = socket_path_for_dir(dir.path());

        let listener = bind_local(&path).await.expect("bind local socket");

        let connect_path = path.clone();
        let connect_task =
            tokio::spawn(async move { connect_local(&connect_path).await.expect("connect") });

        let server_stream = accept_local(&listener).await.expect("accept connection");
        let client_stream = connect_task.await.expect("connect task joined");

        drop(server_stream);
        drop(client_stream);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn connect_local_reports_missing_when_no_listener() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = socket_path_for_dir(dir.path());

        let error = connect_local(&path)
            .await
            .expect_err("should fail with no listener");

        assert!(
            is_connection_missing(&error),
            "missing socket should be connection-missing: {error}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bind_local_removes_stale_socket_and_rebinds() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = socket_path_for_dir(dir.path());

        let first = bind_local(&path).await.expect("first bind");
        drop(first);

        bind_local(&path)
            .await
            .expect("second bind should succeed after stale socket removed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bind_local_refuses_to_replace_live_listener() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = socket_path_for_dir(dir.path());
        let _listener = bind_local(&path).await.expect("first bind");
        let error = bind_local(&path).await.expect_err("live listener must win");
        std::assert_matches!(error, AppError::Io(ref io) if io.kind() == io::ErrorKind::AddrInUse);
    }

    #[test]
    fn host_lock_is_exclusive_until_owner_drops() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let first = acquire_host_lock(dir.path()).expect("first host lock");
        let error = acquire_host_lock(dir.path()).expect_err("second host must be rejected");
        std::assert_matches!(error, AppError::Io(ref io) if io.kind() == io::ErrorKind::AlreadyExists);

        drop(first);
        acquire_host_lock(dir.path()).expect("lock should be reusable after owner exits");
    }

    #[test]
    fn is_connection_missing_covers_expected_kinds() {
        use std::io::ErrorKind;
        for kind in [
            ErrorKind::ConnectionRefused,
            ErrorKind::NotFound,
            ErrorKind::AddrNotAvailable,
        ] {
            assert!(
                is_connection_missing(&io::Error::from(kind)),
                "{kind:?} should be connection-missing"
            );
        }
        assert!(!is_connection_missing(&io::Error::from(
            ErrorKind::PermissionDenied
        )));
        assert!(
            !is_connection_missing(&io::Error::from(ErrorKind::TimedOut)),
            "a connect timeout can mean a live listener with a saturated backlog"
        );
    }
}
