use std::{
    fs::{self, DirBuilder, File, OpenOptions, Permissions},
    io,
    os::unix::fs::{
        DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _,
    },
    path::{Path, PathBuf},
};

use rustix::{
    fs::{Access, AtFlags, CWD, Mode, OFlags, accessat, openat},
    process::geteuid,
};

use super::invalid;

pub(super) fn home_dir() -> io::Result<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot find the home folder"))
}

#[expect(
    clippy::disallowed_methods,
    reason = "kodosi_os::fs::canonicalize owns path canonicalization"
)]
pub(super) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

pub(super) fn default_data_root() -> io::Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().join("kodosi"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot find the home folder"))
}

fn owned(metadata: &fs::Metadata) -> bool {
    metadata.uid() == geteuid().as_raw()
}

pub(super) fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        match fs::symlink_metadata(&prefix) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid(format!(
                    "storage path contains a symlink: {}",
                    prefix.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(invalid(format!(
                    "storage path is not a directory: {}",
                    prefix.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match DirBuilder::new().mode(0o700).create(&prefix) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&prefix)?;
                        if !metadata.is_dir()
                            || metadata.file_type().is_symlink()
                            || !owned(&metadata)
                        {
                            return Err(invalid("storage directory changed during creation"));
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        }
    }
    if !owned(&fs::metadata(path)?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime storage belongs to another user",
        ));
    }
    fs::set_permissions(path, Permissions::from_mode(0o700))
}

pub(super) fn prepare_private_parent(parent: &Path) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("private storage parent is not a directory"));
    }
    fs::set_permissions(parent, Permissions::from_mode(0o700))
}

pub(super) fn restrict_file(file: &File) -> io::Result<()> {
    file.set_permissions(Permissions::from_mode(0o600))
}

pub(super) fn sync_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

pub(super) fn open_private(path: &Path) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if !owned(&metadata)
                || metadata.mode() & 0o077 != 0
                || !metadata.file_type().is_file() =>
        {
            return Err(invalid(format!("unsafe private file: {}", path.display())));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)?;
    if !owned(&file.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "private file belongs to another user",
        ));
    }
    Ok(file)
}

pub(super) fn append_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)
}

pub(super) fn open_dir(path: &Path) -> io::Result<File> {
    File::open(path)
}

pub(super) fn open_beneath(parent: &File, name: &str, directory: bool) -> io::Result<File> {
    let flags = if directory {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    } else {
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK
    };
    Ok(File::from(openat(parent, name, flags, Mode::empty())?))
}

pub(super) fn path_from_file_url(_host: &str, path: &str) -> PathBuf {
    PathBuf::from(path)
}

pub(super) fn can_read(path: &Path) -> bool {
    accessat(CWD, path, Access::READ_OK, AtFlags::EACCESS).is_ok()
}

pub(super) fn can_write(path: &Path) -> bool {
    accessat(CWD, path, Access::WRITE_OK, AtFlags::EACCESS).is_ok()
}

pub(super) fn is_executable(path: &Path) -> bool {
    accessat(CWD, path, Access::EXEC_OK, AtFlags::EACCESS).is_ok()
}
