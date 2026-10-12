use std::{
    fs::File,
    io::{self, Write as _},
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// The user's home folder.
pub fn home_dir() -> io::Result<PathBuf> {
    imp::home_dir()
}

/// Where Kodosi keeps its data when no isolated root is configured.
pub fn default_data_root() -> io::Result<PathBuf> {
    imp::default_data_root()
}

/// Creates `path` as a directory that only the current user can use.
///
/// Missing ancestors are created too. Refuses relative paths, `.` and `..`
/// components, links at any component, non-directories, and a final directory
/// owned by another user.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid(
            "storage path must be absolute without dot components",
        ));
    }
    imp::create_private_dir(path)
}

/// Atomically replaces `path` with `bytes` in a file that only the current
/// user can read, creating the parent folder privately if needed.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("private storage path has no parent"))?;
    imp::prepare_private_parent(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    imp::restrict_file(temporary.as_file())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    imp::sync_dir(parent)
}

/// Opens `path` for reading and writing, creating it private to the current
/// user. Refuses links and files that another user owns.
pub fn open_private(path: &Path) -> io::Result<File> {
    imp::open_private(path)
}

/// Opens `path` for appending, creating it private to the current user.
pub fn append_private(path: &Path) -> io::Result<File> {
    imp::append_private(path)
}

/// Resolves `path` to its absolute form with every link followed, written the
/// way other programs report it. Windows keeps a `\\?\` prefix only for paths
/// that cannot be written without one.
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    imp::canonicalize(path)
}

/// Opens the directory at `path` so files can be opened beneath it.
pub fn open_dir(path: &Path) -> io::Result<File> {
    imp::open_dir(path)
}

/// Opens the directory `name` inside `parent` without following a link.
pub fn open_dir_beneath(parent: &File, name: &str) -> io::Result<File> {
    imp::open_beneath(parent, component(name)?, true)
}

/// Opens the regular file `name` inside `parent` for reading without
/// following a link.
pub fn open_file_beneath(parent: &File, name: &str) -> io::Result<File> {
    let file = imp::open_beneath(parent, component(name)?, false)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("not a regular file"));
    }
    Ok(file)
}

/// The local path named by a `file://` URL's host and percent-decoded path.
pub fn path_from_file_url(host: &str, path: &str) -> PathBuf {
    imp::path_from_file_url(host, path)
}

/// Whether the current user can read the file at `path`.
pub fn can_read(path: &Path) -> bool {
    imp::can_read(path)
}

/// Whether the current user can write the file at `path`.
pub fn can_write(path: &Path) -> bool {
    imp::can_write(path)
}

/// Whether `path` is a file the current user can run.
#[cfg(unix)]
pub(crate) fn is_executable(path: &Path) -> bool {
    path.is_file() && imp::is_executable(path)
}

fn component(name: &str) -> io::Result<&str> {
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return Err(invalid("invalid path component"));
    }
    Ok(name)
}

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests;
