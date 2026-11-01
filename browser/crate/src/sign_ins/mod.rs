//! Copies sign-ins from a person's real browsers into a kept profile, so most sign-ins never
//! happen here. A host starts this only for a person's own action.

mod chromium;
mod firefox;
mod safari;
#[cfg(windows)]
mod windows;

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Error, devtools::Devtools};

const COPIED_FILE: &str = "copied-sign-ins.json";
const WORKING_FOLDER: &str = "sign-in-copy";
const COOKIES_PER_CALL: usize = 500;
/// A date in the past, which removes a cookie.
const GONE: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Chromium(&'static chromium::Family),
    Firefox,
    Safari,
}

/// One browser profile that sign-ins can come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInSource {
    /// What a person reads: the browser, and the profile's name when it has several.
    pub name: String,
    kind: Kind,
    folder: PathBuf,
}

/// What a copy did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Copied {
    pub copied: usize,
    /// Cookies that stay behind, such as those bound to the device.
    pub skipped: usize,
}

/// Why a copy could not start.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("{0} did not give access to its sign-ins.")]
    NoAccess(String),
    #[error("{0} keeps its sign-ins locked on this computer.")]
    Locked(String),
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

impl From<io::Error> for CopyError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::PermissionDenied {
            Self::NoAccess("The browser".to_owned())
        } else {
            Self::Store(error.to_string())
        }
    }
}

/// A cookie as the debug port takes it.
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

/// What one store gave.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Read {
    cookies: Vec<Cookie>,
    skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Key {
    name: String,
    domain: String,
    path: String,
}

fn home() -> PathBuf {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(name).map_or_else(PathBuf::new, PathBuf::from)
}

/// The profiles on this computer that hold sign-ins.
pub fn sign_in_sources() -> Vec<SignInSource> {
    let home = home();
    let mut sources = chromium::sources(&home);
    sources.extend(firefox::sources(&home));
    sources.extend(safari::sources(&home));
    sources
}

/// Reads a store through a copy, as the browser may hold the real one open. Open stores keep
/// their journal beside them.
fn store_copy(store: &Path, working: &Path) -> Result<PathBuf, CopyError> {
    std::fs::create_dir_all(working)?;
    let copy = working.join("store");
    std::fs::copy(store, &copy)?;
    for ending in ["-wal", "-shm", "-journal"] {
        let mut name = store.as_os_str().to_owned();
        name.push(ending);
        let beside = PathBuf::from(name);
        if beside.exists() {
            let mut target = copy.clone().into_os_string();
            target.push(ending);
            std::fs::copy(&beside, PathBuf::from(target))?;
        }
    }
    Ok(copy)
}

fn read_source(source: &SignInSource, working: &Path) -> Result<Read, CopyError> {
    let read = match &source.kind {
        Kind::Chromium(family) => chromium::read(family, &source.folder, working),
        Kind::Firefox => firefox::read(&source.folder, working),
        Kind::Safari => safari::read(&source.folder),
    };
    read.map_err(|error| match error {
        CopyError::NoAccess(_) => CopyError::NoAccess(source.name.clone()),
        CopyError::Locked(_) => CopyError::Locked(source.name.clone()),
        other => other,
    })
}

fn read_keys(profile: &Path) -> Vec<Key> {
    std::fs::read(profile.join(COPIED_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Copies a source's cookies into a running browser, and notes what it copied.
pub(crate) async fn copy(
    devtools: &Devtools,
    profile: &Path,
    source: &SignInSource,
) -> Result<Copied, CopyError> {
    let working = profile.join(WORKING_FOLDER);
    let reader = {
        let source = source.clone();
        let working = working.clone();
        tokio::task::spawn_blocking(move || read_source(&source, &working))
    };
    let read = reader
        .await
        .map_err(|error| CopyError::Store(error.to_string()));
    drop(tokio::fs::remove_dir_all(&working).await);
    let read = read??;
    let mut keys = read_keys(profile);
    for part in read.cookies.chunks(COOKIES_PER_CALL) {
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
    Ok(Copied {
        copied: read.cookies.len(),
        skipped: read.skipped,
    })
}

/// Removes what earlier copies gave a profile, and returns how many cookies went.
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
    Ok(keys.len())
}
