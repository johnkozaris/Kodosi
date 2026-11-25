//! Imports sign-in cookies from installed browsers into a Kodosi profile.
//! Must only be invoked in response to an explicit user action.

mod chromium;
mod firefox;
mod safari;
#[cfg(windows)]
mod windows;

use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags, config::DbConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Error, devtools::Devtools};

const COPIED_FILE: &str = "copied-sign-ins.json";
const COOKIES_PER_CALL: usize = 500;
/// Past Unix timestamp passed to `Storage.setCookies` to expire and delete a cookie.
const GONE: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Chromium(&'static chromium::Family),
    Firefox,
    Safari,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInSource {
    pub name: String,
    pub browser: &'static str,
    /// True when macOS blocks access to this browser's data until Full Disk Access is granted.
    pub protected: bool,
    kind: Kind,
    folder: PathBuf,
}

#[derive(Debug)]
pub struct SignIns {
    cookies: Vec<Cookie>,
}

#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("{0} did not give Kodosi the key of its sign-ins.")]
    NoAccess(String),
    #[error("{0} keeps its sign-ins locked on this computer.")]
    Locked(String),
    #[error("macOS lets only apps with Full Disk Access read the sign-ins of {0}.")]
    Protected(String),
    #[error("The sign-ins could not be read: {0}")]
    Store(String),
    #[error(transparent)]
    Browser(#[from] Error),
}

impl From<rusqlite::Error> for CopyError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Store(error.to_string())
    }
}

/// macOS returns `EPERM` when reading another app's protected data without Full Disk Access.
impl From<io::Error> for CopyError {
    fn from(error: io::Error) -> Self {
        #[cfg(target_os = "macos")]
        if error.raw_os_error() == Some(rustix::io::Errno::PERM.raw_os_error()) {
            return Self::Protected(String::new());
        }
        Self::Store(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Cookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    same_site: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires: Option<f64>,
}

pub(super) const fn same_site_of(value: i64) -> Option<&'static str> {
    match value {
        0 => Some("None"),
        1 => Some("Lax"),
        2 => Some("Strict"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Key {
    name: String,
    domain: String,
    path: String,
}

impl SignInSource {
    fn protected(name: &str, browser: &'static str, kind: Kind, folder: PathBuf) -> Self {
        Self {
            name: name.to_owned(),
            browser,
            protected: true,
            kind,
            folder,
        }
    }

    /// Reads cookies using `working` as a private temporary directory for DB copies, deleting it
    /// on return.
    pub async fn read(&self, working: &Path) -> Result<SignIns, CopyError> {
        let reader = {
            let source = self.clone();
            let working = working.to_owned();
            tokio::task::spawn_blocking(move || read_source(&source, &working))
        };
        let cookies = reader
            .await
            .map_err(|error| CopyError::Store(error.to_string()));
        drop(tokio::fs::remove_dir_all(working).await);
        Ok(SignIns { cookies: cookies?? })
    }
}

fn sites<'a>(domains: impl Iterator<Item = &'a str>) -> usize {
    domains
        .map(|domain| domain.trim_start_matches('.'))
        .collect::<BTreeSet<_>>()
        .len()
}

pub fn copied_sites(profiles: &Path, profile: &str) -> usize {
    let keys = read_keys(&profiles.join(profile));
    sites(keys.iter().map(|key| key.domain.as_str()))
}

/// Lists available browser sources without reading protected app data, avoiding macOS TCC prompts.
pub fn sign_in_sources() -> Vec<SignInSource> {
    let Some(home) = std::env::home_dir().filter(|home| !home.is_empty()) else {
        return Vec::new();
    };
    let mut sources = chromium::sources(&home);
    sources.extend(firefox::sources(&home));
    sources.extend(safari::sources(&home));
    sources
}

/// Checks file readability via `access(R_OK)`, which queries macOS TCC without triggering a
/// permission prompt or logging a denied read.
#[cfg(target_os = "macos")]
fn readable(path: &Path) -> bool {
    rustix::fs::access(path, rustix::fs::Access::READ_OK).is_ok()
}

#[cfg(not(target_os = "macos"))]
fn readable(_path: &Path) -> bool {
    true
}

/// Copies the SQLite DB and its WAL/journal sidecars so we can read stores held open by a browser.
fn store_copy(store: &Path, working: &Path) -> Result<PathBuf, CopyError> {
    std::fs::create_dir_all(working)?;
    let copy = working.join("store");
    copy_data(store, &copy)?;
    for ending in ["-wal", "-shm", "-journal"] {
        let mut name = store.as_os_str().to_owned();
        name.push(ending);
        let beside = PathBuf::from(name);
        if beside.exists() {
            let mut target = copy.clone().into_os_string();
            target.push(ending);
            copy_data(&beside, &PathBuf::from(target))?;
        }
    }
    Ok(copy)
}

/// Opens an external browser's SQLite DB read-only with `TRUSTED_SCHEMA` disabled, since another
/// local process could have modified the schema.
fn open_store(store: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        store,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
    Ok(connection)
}

/// Copies file contents without extended attributes; `std::fs::copy` fails on macOS when copying
/// xattrs from another app's files even if the file itself is readable.
fn copy_data(from: &Path, to: &Path) -> io::Result<()> {
    let mut source = std::fs::File::open(from)?;
    let mut target = std::fs::File::create(to)?;
    io::copy(&mut source, &mut target).map(drop)
}

fn read_source(source: &SignInSource, working: &Path) -> Result<Vec<Cookie>, CopyError> {
    if source.protected {
        return Err(CopyError::Protected(source.name.clone()));
    }
    let cookies = match &source.kind {
        Kind::Chromium(family) => chromium::read(family, &source.folder, working),
        Kind::Firefox => firefox::read(&source.folder, working),
        Kind::Safari => safari::read(&source.folder),
    };
    cookies.map_err(|error| match error {
        CopyError::NoAccess(_) => CopyError::NoAccess(source.name.clone()),
        CopyError::Locked(_) => CopyError::Locked(source.name.clone()),
        CopyError::Protected(_) => CopyError::Protected(source.name.clone()),
        other => other,
    })
}

fn read_keys(profile: &Path) -> Vec<Key> {
    std::fs::read(profile.join(COPIED_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub(crate) async fn add(
    devtools: &Devtools,
    profile: &Path,
    sign_ins: SignIns,
) -> Result<usize, CopyError> {
    let cookies = sign_ins.cookies;
    let mut keys = read_keys(profile);
    for part in cookies.chunks(COOKIES_PER_CALL) {
        devtools
            .call("Storage.setCookies", json!({ "cookies": part }), None)
            .await?;
        keys.extend(part.iter().map(|cookie| Key {
            name: cookie.name.clone(),
            domain: cookie.domain.clone(),
            path: cookie.path.clone(),
        }));
    }
    keys.sort_by(|left, right| {
        (&left.domain, &left.name, &left.path).cmp(&(&right.domain, &right.name, &right.path))
    });
    keys.dedup();
    let bytes = serde_json::to_vec(&keys).map_err(|error| CopyError::Store(error.to_string()))?;
    tokio::fs::write(profile.join(COPIED_FILE), bytes).await?;
    Ok(sites(cookies.iter().map(|cookie| cookie.domain.as_str())))
}

pub(crate) async fn remove(devtools: &Devtools, profile: &Path) -> Result<usize, CopyError> {
    let keys = read_keys(profile);
    for part in keys.chunks(COOKIES_PER_CALL) {
        let cookies: Vec<_> = part
            .iter()
            .map(|key| {
                json!({
                    "name": key.name,
                    "value": "",
                    "domain": key.domain,
                    "path": key.path,
                    "expires": GONE,
                })
            })
            .collect();
        devtools
            .call("Storage.setCookies", json!({ "cookies": cookies }), None)
            .await?;
    }
    if !keys.is_empty() {
        tokio::fs::remove_file(profile.join(COPIED_FILE)).await?;
    }
    Ok(sites(keys.iter().map(|key| key.domain.as_str())))
}
