use std::{fs::File, path::Path};

use crate::{AppError, Result};

#[cfg(unix)]
fn io_error(error: rustix::io::Errno) -> AppError {
    AppError::Io(std::io::Error::from_raw_os_error(error.raw_os_error()))
}

#[cfg(unix)]
pub(super) fn open_read_only(path: &Path) -> Result<File> {
    use rustix::fs::{Mode, OFlags};
    use std::os::fd::OwnedFd;

    let fd = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(io_error)?;
    let fd: OwnedFd = fd;
    Ok(fd.into())
}

#[cfg(unix)]
#[cfg(any(test, feature = "cli"))]
pub(super) fn open_evidence(path: &Path) -> Result<File> {
    use rustix::fs::{Mode, OFlags};
    use std::os::fd::OwnedFd;

    let fd = rustix::fs::open(
        path,
        OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(io_error)?;
    let fd: OwnedFd = fd;
    Ok(fd.into())
}

#[cfg(unix)]
#[cfg(any(test, feature = "cli"))]
pub(super) fn create_evidence(path: &Path) -> Result<File> {
    use rustix::fs::{Mode, OFlags};
    use std::os::fd::OwnedFd;

    let fd = rustix::fs::open(
        path,
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::from_raw_mode(0o600),
    )
    .map_err(io_error)?;
    let fd: OwnedFd = fd;
    Ok(fd.into())
}

#[cfg(unix)]
#[cfg(any(test, feature = "cli"))]
pub(super) fn set_private_permissions(file: &File) -> Result<()> {
    rustix::fs::fchmod(file, rustix::fs::Mode::from_raw_mode(0o600)).map_err(io_error)
}

#[cfg(unix)]
#[cfg(any(test, feature = "cli"))]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(io_error)
}
