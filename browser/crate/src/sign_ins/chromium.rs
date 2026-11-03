use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use aes::Aes128;
#[cfg(unix)]
use cbc::{
    Decryptor,
    cipher::{BlockModeDecrypt as _, KeyIvInit as _, block_padding::Pkcs7},
};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
#[cfg(unix)]
use sha1::Sha1;

use super::{Cookie, CopyError, Kind, SignInSource, store_copy};

#[cfg(unix)]
const SALT: &[u8] = b"saltysalt";
#[cfg(unix)]
const IV: [u8; 16] = [b' '; 16];
#[cfg(unix)]
const KEY_BYTES: usize = 16;
#[cfg(target_os = "macos")]
const KEY_ROUNDS: u32 = 1003;
#[cfg(all(unix, not(target_os = "macos")))]
const KEY_ROUNDS: u32 = 1;
const HOST_HASH_BYTES: usize = 32;
/// From this version of the store, each value starts with a hash of its host.
const HASHED_FROM: i64 = 24;
const SECONDS_TO_1970: f64 = 11_644_473_600.0;
const MICROSECONDS: f64 = 1_000_000.0;
pub(super) const WITH_KEY: &[u8] = b"v10";
#[cfg(unix)]
const WITH_SYSTEM_KEY: &[u8] = b"v11";
const BOUND_TO_APP: &[u8] = b"v20";

/// Where one browser of the Chromium family keeps its profiles, and the name of its secret.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Family {
    name: &'static str,
    mac: &'static str,
    linux: &'static str,
    windows: &'static str,
    secret: &'static str,
    account: &'static str,
    application: &'static str,
}

static FAMILIES: [Family; 6] = [
    Family {
        name: "Chrome",
        mac: "Google/Chrome",
        linux: "google-chrome",
        windows: "Google/Chrome/User Data",
        secret: "Chrome Safe Storage",
        account: "Chrome",
        application: "chrome",
    },
    Family {
        name: "Edge",
        mac: "Microsoft Edge",
        linux: "microsoft-edge",
        windows: "Microsoft/Edge/User Data",
        secret: "Microsoft Edge Safe Storage",
        account: "Microsoft Edge",
        application: "chromium",
    },
    Family {
        name: "Brave",
        mac: "BraveSoftware/Brave-Browser",
        linux: "BraveSoftware/Brave-Browser",
        windows: "BraveSoftware/Brave-Browser/User Data",
        secret: "Brave Safe Storage",
        account: "Brave",
        application: "brave",
    },
    Family {
        name: "Chromium",
        mac: "Chromium",
        linux: "chromium",
        windows: "Chromium/User Data",
        secret: "Chromium Safe Storage",
        account: "Chromium",
        application: "chromium",
    },
    Family {
        name: "Arc",
        mac: "Arc/User Data",
        linux: "",
        windows: "",
        secret: "Arc Safe Storage",
        account: "Arc",
        application: "chromium",
    },
    Family {
        name: "Vivaldi",
        mac: "Vivaldi",
        linux: "vivaldi",
        windows: "Vivaldi/User Data",
        secret: "Vivaldi Safe Storage",
        account: "Vivaldi",
        application: "vivaldi",
    },
];

#[derive(Deserialize, Default)]
struct LocalState {
    #[serde(default)]
    profile: ProfileList,
}

#[derive(Deserialize, Default)]
struct ProfileList {
    #[serde(default)]
    info_cache: BTreeMap<String, ProfileInfo>,
}

#[derive(Deserialize)]
struct ProfileInfo {
    #[serde(default)]
    name: String,
}

fn root_of(family: &Family, home: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return Some(home.join("Library/Application Support").join(family.mac));
    }
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA")?;
        return (!family.windows.is_empty()).then(|| PathBuf::from(local).join(family.windows));
    }
    (!family.linux.is_empty()).then(|| home.join(".config").join(family.linux))
}

fn store_of(profile: &Path) -> Option<PathBuf> {
    [profile.join("Network/Cookies"), profile.join("Cookies")]
        .into_iter()
        .find(|store| store.is_file())
}

/// Each profile of each browser of the family on this computer.
pub(super) fn sources(home: &Path) -> Vec<SignInSource> {
    let mut sources = Vec::new();
    for family in &FAMILIES {
        let Some(root) = root_of(family, home) else {
            continue;
        };
        let state: LocalState = std::fs::read(root.join("Local State"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut profiles: Vec<(String, String)> = state
            .profile
            .info_cache
            .into_iter()
            .map(|(folder, info)| (folder, info.name))
            .collect();
        if profiles.is_empty() {
            profiles.push(("Default".to_owned(), String::new()));
        }
        let several = profiles.len() > 1;
        for (folder, name) in profiles {
            let folder = root.join(folder);
            if store_of(&folder).is_none() {
                continue;
            }
            sources.push(SignInSource {
                name: if several && !name.is_empty() {
                    format!("{}: {name}", family.name)
                } else {
                    family.name.to_owned()
                },
                kind: Kind::Chromium(family),
                folder,
            });
        }
    }
    sources
}

/// What opens the values of a store.
pub(crate) trait Opener {
    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>>;
}

/// The keys of a store on a Mac or on Linux.
#[cfg(unix)]
#[derive(Default)]
struct SealKeys {
    with_key: Option<[u8; KEY_BYTES]>,
    with_system_key: Option<[u8; KEY_BYTES]>,
}

#[cfg(unix)]
fn key_of(password: &[u8]) -> [u8; KEY_BYTES] {
    let mut key = [0; KEY_BYTES];
    pbkdf2::pbkdf2_hmac::<Sha1>(password, SALT, KEY_ROUNDS, &mut key);
    key
}

#[cfg(unix)]
fn opened(key: &[u8; KEY_BYTES], sealed: &[u8]) -> Option<Vec<u8>> {
    let mut buffer = sealed.to_vec();
    let plain = Decryptor::<Aes128>::new(key.into(), (&IV).into())
        .decrypt_padded::<Pkcs7>(&mut buffer)
        .ok()?;
    Some(plain.to_vec())
}

#[cfg(unix)]
impl Opener for SealKeys {
    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if let Some(rest) = sealed.strip_prefix(WITH_KEY) {
            return opened(self.with_key.as_ref()?, rest);
        }
        if let Some(rest) = sealed.strip_prefix(WITH_SYSTEM_KEY) {
            return opened(self.with_system_key.as_ref()?, rest);
        }
        None
    }
}

#[cfg(target_os = "macos")]
fn opener_of(family: &Family, _folder: &Path) -> Result<Box<dyn Opener>, CopyError> {
    let password =
        security_framework::passwords::get_generic_password(family.secret, family.account)
            .map_err(|_| CopyError::NoAccess(family.name.to_owned()))?;
    Ok(Box::new(SealKeys {
        with_key: Some(key_of(&password)),
        with_system_key: None,
    }))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_password(application: &str) -> Option<Vec<u8>> {
    use secret_service::{EncryptionType, SecretService};

    let runtime = tokio::runtime::Handle::try_current().ok()?;
    runtime.block_on(async {
        let service = SecretService::connect(EncryptionType::Dh).await.ok()?;
        let found = service
            .search_items(std::collections::HashMap::from([(
                "application",
                application,
            )]))
            .await
            .ok()?;
        let item = found.unlocked.first()?;
        item.get_secret().await.ok()
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same signature as on the other systems"
)]
fn opener_of(family: &Family, _folder: &Path) -> Result<Box<dyn Opener>, CopyError> {
    Ok(Box::new(SealKeys {
        with_key: Some(key_of(b"peanuts")),
        with_system_key: system_password(family.application).map(|password| key_of(&password)),
    }))
}

#[cfg(windows)]
fn opener_of(family: &Family, folder: &Path) -> Result<Box<dyn Opener>, CopyError> {
    crate::sign_ins::windows::opener(family.name, folder)
}

fn expiry_of(expires: i64) -> Option<f64> {
    #[expect(clippy::cast_precision_loss, reason = "a date in microseconds")]
    (expires > 0).then(|| expires as f64 / MICROSECONDS - SECONDS_TO_1970)
}

fn same_site_of(value: i64) -> Option<&'static str> {
    match value {
        0 => Some("None"),
        1 => Some("Lax"),
        2 => Some("Strict"),
        _ => None,
    }
}

/// The cookies that a device-bound session holds. Its record names them in plain bytes.
#[derive(Default)]
struct BoundCookies {
    sessions: Vec<(String, Vec<u8>)>,
}

impl BoundCookies {
    fn read(profile: &Path) -> Self {
        let store = profile.join("Device Bound Sessions");
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let Ok(connection) = Connection::open_with_flags(store, flags) else {
            return Self::default();
        };
        let sessions = bound_rows(&connection).unwrap_or_default();
        Self { sessions }
    }

    fn holds(&self, host: &str, name: &str) -> bool {
        let host = host.trim_start_matches('.');
        self.sessions.iter().any(|(site, record)| {
            let site = site.rsplit("//").next().unwrap_or(site);
            (host == site || host.ends_with(&format!(".{site}")))
                && record
                    .windows(name.len().max(1))
                    .any(|part| part == name.as_bytes())
        })
    }
}

fn bound_rows(connection: &Connection) -> rusqlite::Result<Vec<(String, Vec<u8>)>> {
    let table: String = connection.query_row(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE '%session_proto%'",
        [],
        |row| row.get(0),
    )?;
    let quoted = table.replace('"', "\"\"");
    let mut statement =
        connection.prepare(&format!(r#"SELECT site, session_proto FROM "{quoted}""#))?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

struct Row {
    host: String,
    name: String,
    value: String,
    sealed: Vec<u8>,
    path: String,
    expires: i64,
    secure: bool,
    http_only: bool,
    same_site: i64,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

/// The value of a row, when this computer can open it.
fn value_of(row: &Row, opener: &dyn Opener, hashed: bool) -> Option<String> {
    if row.sealed.is_empty() {
        return Some(row.value.clone());
    }
    if row.sealed.starts_with(BOUND_TO_APP) {
        return None;
    }
    let plain = opener.open(&row.sealed)?;
    let plain = if hashed {
        plain.get(HOST_HASH_BYTES..)?
    } else {
        &plain
    };
    String::from_utf8(plain.to_vec()).ok()
}

fn read_store(
    store: &Path,
    opener: &dyn Opener,
    bound: &BoundCookies,
) -> Result<Vec<Cookie>, CopyError> {
    let connection = Connection::open_with_flags(store, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: i64 = connection
        .query_row("SELECT value FROM meta WHERE key = 'version'", [], |row| {
            row.get::<_, String>(0)
                .map(|text| text.parse().unwrap_or(0))
        })
        .unwrap_or(0);
    let mut statement = connection.prepare(
        "SELECT host_key, name, value, encrypted_value, path, expires_utc, is_secure, \
         is_httponly, samesite FROM cookies",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(Row {
            host: row.get(0)?,
            name: row.get(1)?,
            value: row.get(2)?,
            sealed: row.get(3)?,
            path: row.get(4)?,
            expires: row.get(5)?,
            secure: row.get(6)?,
            http_only: row.get(7)?,
            same_site: row.get(8)?,
        })
    })?;
    let mut cookies = Vec::new();
    let today = now();
    for row in rows {
        let row = row?;
        let expires = expiry_of(row.expires);
        if expires.is_some_and(|expires| expires < today) {
            continue;
        }
        let value = (!bound.holds(&row.host, &row.name))
            .then(|| value_of(&row, opener, version >= HASHED_FROM))
            .flatten();
        let Some(value) = value else {
            continue;
        };
        cookies.push(Cookie {
            name: row.name,
            value,
            domain: row.host,
            path: row.path,
            secure: row.secure,
            http_only: row.http_only,
            same_site: same_site_of(row.same_site),
            expires,
        });
    }
    Ok(cookies)
}

pub(super) fn read(
    family: &Family,
    folder: &Path,
    working: &Path,
) -> Result<Vec<Cookie>, CopyError> {
    let store = store_of(folder).ok_or_else(|| CopyError::Store("no cookies".to_owned()))?;
    let opener = opener_of(family, folder)?;
    let copy = store_copy(&store, working).map_err(|error| match error {
        CopyError::Store(_) if cfg!(windows) => CopyError::Locked(family.name.to_owned()),
        other => other,
    })?;
    read_store(&copy, opener.as_ref(), &BoundCookies::read(folder))
}

#[cfg(all(test, unix))]
mod tests {
    use cbc::{Encryptor, cipher::BlockModeEncrypt as _};
    use sha2::{Digest as _, Sha256};

    use super::*;

    const FAR_FUTURE: i64 = 20_000_000_000_000_000;
    const FAR_PAST: i64 = 10_000_000_000_000_000;

    fn sealed(key: &[u8; KEY_BYTES], prefix: &[u8], plain: &[u8]) -> Vec<u8> {
        let mut buffer = vec![0; plain.len() + KEY_BYTES];
        let written = Encryptor::<Aes128>::new(key.into(), (&IV).into())
            .encrypt_padded_b2b::<Pkcs7>(plain, &mut buffer)
            .unwrap();
        [prefix, written].concat()
    }

    fn store(folder: &Path, version: u32, rows: &[(&str, &str, &str, &[u8], i64)]) -> PathBuf {
        let path = folder.join("Cookies");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE meta(key TEXT, value TEXT);
                 INSERT INTO meta VALUES('version', '{version}');
                 CREATE TABLE cookies(host_key TEXT, name TEXT, value TEXT, encrypted_value BLOB,
                   path TEXT, expires_utc INTEGER, is_secure INTEGER, is_httponly INTEGER,
                   samesite INTEGER);"
            ))
            .unwrap();
        for (host, name, value, sealed, expires) in rows {
            connection
                .execute(
                    "INSERT INTO cookies VALUES(?1, ?2, ?3, ?4, '/', ?5, 1, 1, 1)",
                    (host, name, value, sealed, expires),
                )
                .unwrap();
        }
        path
    }

    fn keys() -> ([u8; KEY_BYTES], SealKeys) {
        let key = key_of(b"not a real password");
        (
            key,
            SealKeys {
                with_key: Some(key),
                with_system_key: None,
            },
        )
    }

    #[test]
    fn sealed_values_open_and_plain_values_stay() {
        let folder = tempfile::tempdir().unwrap();
        let (key, opener) = keys();
        let seal = sealed(&key, WITH_KEY, b"token");
        let path = store(
            folder.path(),
            23,
            &[
                (".site.test", "a", "", &seal, FAR_FUTURE),
                (".site.test", "b", "plain", b"", 0),
            ],
        );
        let cookies = read_store(&path, &opener, &BoundCookies::default()).unwrap();
        assert_eq!(cookies[0].value, "token");
        assert_eq!(cookies[0].same_site, Some("Lax"));
        assert!(cookies[0].http_only);
        assert_eq!(cookies[1].value, "plain");
        assert_eq!(cookies[1].expires, None);
    }

    #[test]
    fn a_new_store_has_a_hash_of_the_host_before_each_value() {
        let folder = tempfile::tempdir().unwrap();
        let (key, opener) = keys();
        let hash = Sha256::digest(".site.test");
        let plain = [hash.as_slice(), b"token"].concat();
        let seal = sealed(&key, WITH_KEY, &plain);
        let path = store(
            folder.path(),
            24,
            &[(".site.test", "a", "", &seal, FAR_FUTURE)],
        );
        let cookies = read_store(&path, &opener, &BoundCookies::default()).unwrap();
        assert_eq!(cookies[0].value, "token");
    }

    #[test]
    fn what_cannot_open_is_skipped_and_what_expired_goes() {
        let folder = tempfile::tempdir().unwrap();
        let (key, opener) = keys();
        let other = key_of(b"another password");
        let wrong = sealed(&other, WITH_KEY, b"x");
        let bound = [BOUND_TO_APP, b"anything"].concat();
        let old = sealed(&key, WITH_KEY, b"old");
        let path = store(
            folder.path(),
            23,
            &[
                (".site.test", "wrong", "", &wrong, FAR_FUTURE),
                (".site.test", "app", "", &bound, FAR_FUTURE),
                (".site.test", "old", "", &old, FAR_PAST),
            ],
        );
        let cookies = read_store(&path, &opener, &BoundCookies::default()).unwrap();
        assert_eq!(cookies, Vec::new());
    }

    #[test]
    fn a_cookie_of_a_device_bound_session_stays_behind() {
        let folder = tempfile::tempdir().unwrap();
        let (_key, opener) = keys();
        let path = store(
            folder.path(),
            23,
            &[
                (
                    ".accounts.site.test",
                    "__Secure-bound",
                    "x",
                    b"",
                    FAR_FUTURE,
                ),
                (".accounts.site.test", "free", "y", b"", FAR_FUTURE),
                (".other.test", "__Secure-bound", "z", b"", FAR_FUTURE),
            ],
        );
        let bound = BoundCookies {
            sessions: vec![(
                "https://site.test".to_owned(),
                b"\x0a\x0e__Secure-bound\x12".to_vec(),
            )],
        };
        let cookies = read_store(&path, &opener, &bound).unwrap();
        let names: Vec<_> = cookies
            .iter()
            .map(|cookie| (cookie.domain.as_str(), cookie.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (".accounts.site.test", "free"),
                (".other.test", "__Secure-bound")
            ]
        );
    }

    #[test]
    fn a_profile_shows_under_its_name_when_a_browser_has_several() {
        let home = tempfile::tempdir().unwrap();
        let Some(root) = root_of(&FAMILIES[0], home.path()) else {
            return;
        };
        for profile in ["Default", "Profile 1"] {
            std::fs::create_dir_all(root.join(profile)).unwrap();
            std::fs::write(root.join(profile).join("Cookies"), b"").unwrap();
        }
        std::fs::write(
            root.join("Local State"),
            br#"{"profile":{"info_cache":{"Default":{"name":"Home"},"Profile 1":{"name":"Work"}}}}"#,
        )
        .unwrap();
        let names: Vec<String> = sources(home.path())
            .into_iter()
            .map(|source| source.name)
            .collect();
        assert_eq!(names, ["Chrome: Home", "Chrome: Work"]);
    }
}
