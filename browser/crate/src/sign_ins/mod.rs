//! Copies sign-ins from a person's real browsers into a kept profile, so most sign-ins never
//! happen here. A host starts this only for a person's own action.

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

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Error, devtools::Devtools};

const COPIED_FILE: &str = "copied-sign-ins.json";
const COOKIES_PER_CALL: usize = 500;
/// A date in the past, which removes a cookie.
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
    /// macOS keeps the data of the browser from Kodosi until the person gives Kodosi Full Disk
    /// Access, so its profiles are not known yet.
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

/// macOS refuses the files of another app's data with `EPERM` until the person gives Kodosi
/// Full Disk Access.
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

    /// Copies of open stores go in `working`, a private folder that goes when the read ends.
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

fn home() -> PathBuf {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(name).map_or_else(PathBuf::new, PathBuf::from)
}

/// Nothing here reads the data of a browser that macOS keeps from Kodosi, so looking never makes
/// macOS refuse or warn.
pub fn sign_in_sources() -> Vec<SignInSource> {
    let home = home();
    let mut sources = chromium::sources(&home);
    sources.extend(firefox::sources(&home));
    sources.extend(safari::sources(&home));
    sources
}

/// Whether Kodosi may read a file that is there. macOS keeps the data of other apps from Kodosi
/// until the person gives it Full Disk Access, and answers this without asking the person and
/// without counting it as a refused read.
#[cfg(target_os = "macos")]
fn readable(path: &Path) -> bool {
    rustix::fs::access(path, rustix::fs::Access::READ_OK).is_ok()
}

#[cfg(not(target_os = "macos"))]
fn readable(_path: &Path) -> bool {
    true
}

/// Reads a store through a copy, as the browser may hold the real one open. Open stores keep
/// their journal beside them.
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

/// Copies only the bytes of a file. macOS refuses a copy of the extended attributes of another
/// app's file, even where it lets Kodosi read the file.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_count_each_site_once() {
        let profiles = tempfile::tempdir().unwrap();
        assert_eq!(copied_sites(profiles.path(), "person"), 0);
        let key = |name: &str, domain: &str| Key {
            name: name.to_owned(),
            domain: domain.to_owned(),
            path: "/".to_owned(),
        };
        let keys = [
            key("a", ".site.test"),
            key("b", "site.test"),
            key("c", "mail.site.test"),
        ];
        let profile = profiles.path().join("person");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(
            profile.join(COPIED_FILE),
            serde_json::to_vec(&keys).unwrap(),
        )
        .unwrap();
        assert_eq!(copied_sites(profiles.path(), "person"), 2);
    }
}
