#![allow(unsafe_code)]

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::{
        fs::{MetadataExt as _, OpenOptionsExt as _},
        io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr,
};

use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_FOR_BACKUP_INTENT,
            FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
        },
    },
    Win32::{
        Foundation::{
            HANDLE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, RtlNtStatusToDosError, UNICODE_STRING,
        },
        Security::{
            Authorization::{SE_FILE_OBJECT, SetSecurityInfo},
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        },
        Storage::FileSystem::{
            CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_LIST_DIRECTORY,
            FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FILE_TRAVERSE, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

use super::invalid;
use crate::win::{self, Descriptor};

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

pub(super) fn home_dir() -> io::Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot find the home folder"))
}

pub(super) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    dunce::canonicalize(path)
}

pub(super) fn default_data_root() -> io::Result<PathBuf> {
    Ok(home_dir()?.join(".kodosi"))
}

fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn private_folder() -> io::Result<Descriptor> {
    Descriptor::private("FA", true, true)
}

pub(super) fn create_private_dir(path: &Path) -> io::Result<()> {
    let descriptor = private_folder()?;
    let attributes = descriptor.attributes();
    let mut prefixes: Vec<&Path> = path
        .ancestors()
        .filter(|prefix| prefix.file_name().is_some())
        .collect();
    prefixes.reverse();
    for prefix in prefixes {
        match fs::symlink_metadata(prefix) {
            Ok(metadata) if is_link(&metadata) => {
                return Err(invalid(format!(
                    "storage path contains a link: {}",
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
                if unsafe { CreateDirectoryW(win::wide(prefix).as_ptr(), &raw const attributes) }
                    == 0
                {
                    let error = io::Error::last_os_error();
                    if error.kind() != io::ErrorKind::AlreadyExists {
                        return Err(error);
                    }
                    let metadata = fs::symlink_metadata(prefix)?;
                    if is_link(&metadata) || !metadata.is_dir() {
                        return Err(invalid("storage directory changed during creation"));
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
    let directory = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES)
        .share_mode(SHARE_ALL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = directory.metadata()?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(invalid("storage directory changed during creation"));
    }
    if !win::owned_by_user(directory.as_raw_handle(), SE_FILE_OBJECT)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime storage belongs to another user",
        ));
    }
    let status = unsafe {
        SetSecurityInfo(
            directory.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            descriptor.dacl()?,
            ptr::null(),
        )
    };
    if status != 0 {
        return Err(win::win32_error(status));
    }
    Ok(())
}

pub(super) fn prepare_private_parent(parent: &Path) -> io::Result<()> {
    fs::create_dir_all(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(invalid("private storage parent is not a directory"));
    }
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "matches the Unix version, which sets permissions"
)]
pub(super) const fn restrict_file(_file: &File) -> io::Result<()> {
    // New files inherit the private folder's protected access list.
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "matches the Unix version, which flushes the folder"
)]
pub(super) const fn sync_dir(_path: &Path) -> io::Result<()> {
    // NTFS journals the rename; Windows has no directory flush for callers.
    Ok(())
}

fn checked_file(file: File) -> io::Result<File> {
    let metadata = file.metadata()?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(invalid("private file is a link or not a regular file"));
    }
    Ok(file)
}

pub(super) fn open_private(path: &Path) -> io::Result<File> {
    let file = checked_file(
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?,
    )?;
    if !win::owned_by_user(file.as_raw_handle(), SE_FILE_OBJECT)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "private file belongs to another user",
        ));
    }
    Ok(file)
}

pub(super) fn read_private(path: &Path) -> io::Result<Option<zeroize::Zeroizing<Vec<u8>>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(zeroize::Zeroizing::new(bytes))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn append_private(path: &Path) -> io::Result<File> {
    checked_file(
        OpenOptions::new()
            .create(true)
            .append(true)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?,
    )
}

pub(super) fn open_dir(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(SHARE_ALL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

pub(super) fn open_beneath(parent: &File, name: &str, directory: bool) -> io::Result<File> {
    let name: Vec<u16> = name.encode_utf16().collect();
    let length =
        u16::try_from(name.len() * 2).map_err(|_| invalid("path component is too long"))?;
    let object_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).unwrap_or(u32::MAX),
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &raw const object_name,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let (access, options) = if directory {
        (
            FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_FOR_BACKUP_INTENT,
        )
    } else {
        (
            FILE_GENERIC_READ,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
        )
    };
    let mut handle: HANDLE = ptr::null_mut();
    let mut status_block: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            access,
            &raw const attributes,
            &raw mut status_block,
            ptr::null(),
            0,
            SHARE_ALL,
            FILE_OPEN,
            options,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(win::win32_error(unsafe { RtlNtStatusToDosError(status) }));
    }
    Ok(File::from(unsafe { OwnedHandle::from_raw_handle(handle) }))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the Unix form refuses the folders of other computers, and both forms share one signature"
)]
pub(super) fn path_from_file_url(host: &str, path: &str) -> Option<PathBuf> {
    let bytes = path.as_bytes();
    let path = path.replace('/', "\\");
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        return Some(PathBuf::from(&path[1..]));
    }
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return Some(PathBuf::from(format!(r"\\{host}{path}")));
    }
    Some(PathBuf::from(path))
}

pub(super) fn can_read(path: &Path) -> bool {
    File::open(path).is_ok()
}

pub(super) fn can_write(path: &Path) -> bool {
    OpenOptions::new().write(true).open(path).is_ok()
}
