use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::{Cookie, CopyError, Kind, SignInSource, store_copy};

const STORE: &str = "cookies.sqlite";
const MILLISECONDS: f64 = 1000.0;
/// A date that is larger than this counts in milliseconds, as newer stores do.
const LARGEST_DATE_IN_SECONDS: i64 = 100_000_000_000;

fn roots(home: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        return vec![home.join("Library/Application Support/Firefox")];
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .map(|data| PathBuf::from(data).join("Mozilla/Firefox"))
            .into_iter()
            .collect();
    }
    vec![
        home.join(".mozilla/firefox"),
        home.join("snap/firefox/common/.mozilla/firefox"),
    ]
}

/// The folders that `profiles.ini` names, the default one first.
fn profile_folders(root: &Path) -> Vec<PathBuf> {
    let Ok(list) = std::fs::read_to_string(root.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut named = Vec::new();
    let mut first = Vec::new();
    let mut path = None;
    let mut relative = true;
    let mut flush = |path: &mut Option<String>, relative: bool, is_install: bool| {
        if let Some(folder) = path.take() {
            let folder = if relative {
                root.join(folder)
            } else {
                PathBuf::from(folder)
            };
            if is_install {
                first.push(folder);
            } else {
                named.push(folder);
            }
        }
    };
    let mut in_install = false;
    for line in list.lines().map(str::trim) {
        if line.starts_with('[') {
            flush(&mut path, relative, in_install);
            in_install = line.starts_with("[Install");
            relative = true;
        } else if let Some(value) = line.strip_prefix("Default=").filter(|_| in_install) {
            path = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("Path=") {
            path = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("IsRelative=") {
            relative = value == "1";
        }
    }
    flush(&mut path, relative, in_install);
    first.extend(named);
    first.dedup();
    first
}

pub(super) fn sources(home: &Path) -> Vec<SignInSource> {
    roots(home)
        .iter()
        .flat_map(|root| profile_folders(root))
        .find(|folder| folder.join(STORE).is_file())
        .map(|folder| SignInSource {
            name: "Firefox".to_owned(),
            kind: Kind::Firefox,
            folder,
        })
        .into_iter()
        .collect()
}

fn expiry_of(expiry: i64) -> Option<f64> {
    if expiry <= 0 {
        return None;
    }
    #[expect(clippy::cast_precision_loss, reason = "a date")]
    let seconds = if expiry > LARGEST_DATE_IN_SECONDS {
        expiry as f64 / MILLISECONDS
    } else {
        expiry as f64
    };
    Some(seconds)
}

fn same_site_of(value: i64) -> Option<&'static str> {
    match value {
        0 => Some("None"),
        1 => Some("Lax"),
        2 => Some("Strict"),
        _ => None,
    }
}

fn read_store(store: &Path) -> Result<Vec<Cookie>, CopyError> {
    let connection = Connection::open_with_flags(store, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = connection.prepare(
        "SELECT name, value, host, path, expiry, isSecure, isHttpOnly, sameSite, \
         originAttributes FROM moz_cookies",
    )?;
    let rows = statement.query_map([], |row| {
        let attributes: String = row.get(8)?;
        Ok((
            Cookie {
                name: row.get(0)?,
                value: row.get(1)?,
                domain: row.get(2)?,
                path: row.get(3)?,
                expires: expiry_of(row.get(4)?),
                secure: row.get(5)?,
                http_only: row.get(6)?,
                same_site: same_site_of(row.get(7)?),
            },
            attributes,
        ))
    })?;
    let mut cookies = Vec::new();
    for row in rows {
        let (cookie, attributes) = row?;
        if attributes.is_empty() {
            cookies.push(cookie);
        }
    }
    Ok(cookies)
}

pub(super) fn read(folder: &Path, working: &Path) -> Result<Vec<Cookie>, CopyError> {
    read_store(&store_copy(&folder.join(STORE), working)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE moz_cookies(name TEXT, value TEXT, host TEXT, path TEXT,
                   expiry INTEGER, isSecure INTEGER, isHttpOnly INTEGER, sameSite INTEGER,
                   originAttributes TEXT);
                 INSERT INTO moz_cookies VALUES('a', '1', '.site.test', '/', 4000000000, 1, 1, 1, '');
                 INSERT INTO moz_cookies VALUES('b', '2', '.site.test', '/', 4000000000000, 0, 0, 2, '');
                 INSERT INTO moz_cookies VALUES('c', '3', '.site.test', '/', 4000000000, 0, 0, 0,
                   '^userContextId=1');",
            )
            .unwrap();
    }

    #[test]
    fn cookies_read_in_both_units_and_container_cookies_stay() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(STORE);
        store(&path);
        let cookies = read_store(&path).unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].expires, cookies[1].expires);
        assert_eq!(cookies[0].same_site, Some("Lax"));
        assert_eq!(cookies[1].same_site, Some("Strict"));
    }

    #[test]
    fn the_default_profile_of_an_install_comes_first() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("profiles.ini"),
            "[Profile1]\nName=old\nIsRelative=1\nPath=Profiles/old\n\n\
             [Install308046B0AF4A39CB]\nDefault=Profiles/main\nLocked=1\n",
        )
        .unwrap();
        assert_eq!(
            profile_folders(root.path()),
            [
                root.path().join("Profiles/main"),
                root.path().join("Profiles/old")
            ]
        );
    }
}
