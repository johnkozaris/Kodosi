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

pub fn home_dir() -> io::Result<PathBuf> {
    // std passes over an empty HOME but gives an empty home folder of the user database as it
    // is, and an empty path would put the data root in the working folder.
    std::env::home_dir()
        .filter(|home| !home.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot find the home folder"))
}

pub fn default_data_root() -> io::Result<PathBuf> {
    imp::default_data_root()
}

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

pub fn replace_keeping_permissions(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("a replaced file has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    imp::sync_dir(parent)
}

pub fn open_private(path: &Path) -> io::Result<File> {
    imp::open_private(path)
}

pub fn read_private(path: &Path) -> io::Result<Option<zeroize::Zeroizing<Vec<u8>>>> {
    imp::read_private(path)
}

pub fn append_private(path: &Path) -> io::Result<File> {
    imp::append_private(path)
}

pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    imp::canonicalize(path)
}

pub fn open_dir(path: &Path) -> io::Result<File> {
    imp::open_dir(path)
}

pub fn open_dir_beneath(parent: &File, name: &str) -> io::Result<File> {
    imp::open_beneath(parent, component(name)?, true)
}

pub fn open_file_beneath(parent: &File, name: &str) -> io::Result<File> {
    let file = imp::open_beneath(parent, component(name)?, false)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("not a regular file"));
    }
    Ok(file)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let trash = {
        let mut trash = trash::TrashContext::default();
        // Finder's way makes macOS ask the person to let Kodosi control Finder.
        trash::macos::TrashContextExtMacos::set_delete_method(
            &mut trash,
            trash::macos::DeleteMethod::NsFileManager,
        );
        trash
    };
    #[cfg(not(target_os = "macos"))]
    let trash = trash::TrashContext::default();
    trash.delete(path).map_err(io::Error::other)
}

pub fn path_from_file_url(host: &str, path: &str) -> Option<PathBuf> {
    imp::path_from_file_url(host, path)
}

pub fn can_read(path: &Path) -> bool {
    imp::can_read(path)
}

pub fn can_write(path: &Path) -> bool {
    imp::can_write(path)
}

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
