use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Once},
    time::Duration,
};

use aws_lc_rs::{
    digest::{Context, SHA256},
    signature::{ED25519, UnparsedPublicKey},
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use shared_browser::{Browsers, Settings};
use tokio::{io::AsyncWriteExt as _, sync::OnceCell};

use super::browser::KEPT_PROFILE;
use crate::text::Text;

const ENGINE_PROGRAM_VARIABLE: &str = "KODOSI_BROWSER_ENGINE";
const LINK: u32 = shared_browser::link::VERSION;
/// Base URL for browser update feeds; each `LINK` version uses `{FEEDS}/{LINK}/engine.json`.
const FEEDS: &str = "https://updates.kodosi.com/browser";
/// Base64 Ed25519 public key (shared with Kodosi app updates) that signs the browser feed.
const FEED_KEY: &str = "6eh21KvaRNcq4kfPMxINLLm+OsMXLgIT6VWfYlU3fow=";
const MOST_FEED: usize = 64 * 1024;
const MOST_ARCHIVE: u64 = 1024 * 1024 * 1024;
const LOOK_LIMIT: Duration = Duration::from_secs(30);
/// Poll daily to pick up Chromium security releases promptly.
const LOOK_EVERY: Duration = Duration::from_hours(24);
fn no_engine() -> Text {
    text!("Kodosi could not get its browser. Check the network and open it again.")
}

fn not_for_this_computer() -> Text {
    text!("Kodosi has no browser for this computer yet.")
}

const CURRENT: &str = "current.json";
const READING: &str = "sign-in-copy";

#[derive(Debug, Deserialize)]
struct Feed {
    version: String,
    link: u32,
    engines: HashMap<String, Engine>,
}

#[derive(Debug, Deserialize)]
struct Engine {
    url: String,
    sha256: String,
    program: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Current {
    version: String,
    program: String,
    link: u32,
}

pub(crate) struct BrowserStatus {
    pub(crate) sources: Vec<shared_browser::SignInSource>,
    pub(crate) sites: usize,
    pub(crate) engine: Option<String>,
    pub(crate) getting: Option<u8>,
}

#[derive(Clone)]
pub(crate) struct BrowserHost {
    folder: PathBuf,
    /// When true, avoid touching the OS keychain (used for isolated data roots and tests).
    isolated: bool,
    browsers: Arc<OnceCell<Browsers>>,
    updating: Arc<Once>,
    /// Serializes engine downloads and cleanup so concurrent starts do not clobber each other.
    installing: Arc<tokio::sync::Mutex<()>>,
    getting: Arc<Mutex<Option<u8>>>,
}

impl BrowserHost {
    pub(crate) fn new(data_root: &Path, isolated: bool) -> Self {
        Self {
            folder: data_root.join("browser"),
            isolated,
            browsers: Arc::default(),
            updating: Arc::default(),
            installing: Arc::default(),
            getting: Arc::default(),
        }
    }

    pub(super) async fn browsers(&self, progress: impl Fn(u8) + Send) -> Result<&Browsers, Text> {
        let browsers = self
            .browsers
            .get_or_try_init(|| async {
                let engine = match on_disk() {
                    Some(program) => program,
                    None => self.engine(progress).await?,
                };
                Ok::<_, Text>(Browsers::new(Settings {
                    engine,
                    profiles: self.profiles(),
                    system_keychain: !self.isolated,
                }))
            })
            .await?;
        if on_disk().is_none() {
            self.updating.call_once(|| self.keep_current());
        }
        Ok(browsers)
    }

    async fn engine(&self, progress: impl Fn(u8) + Send) -> Result<PathBuf, Text> {
        let _one = self.installing.lock().await;
        if let Some((version, program)) = installed(&self.engines()).await {
            remove_others(&self.engines(), &version).await;
            return Ok(program);
        }
        self.download(progress).await
    }

    /// Caller must hold `self.installing`.
    async fn download(&self, progress: impl Fn(u8) + Send) -> Result<PathBuf, Text> {
        let got = install(&self.engines(), move |part| {
            self.tell(Some(part));
            progress(part);
        })
        .await;
        self.tell(None);
        got
    }

    async fn newest(&self) -> Result<PathBuf, Text> {
        let _one = self.installing.lock().await;
        self.download(|_| {}).await
    }

    #[cfg(test)]
    pub(super) async fn hold_downloads(&self) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        if on_disk().is_some() {
            return None;
        }
        Some(Arc::clone(&self.installing).lock_owned().await)
    }

    fn tell(&self, part: Option<u8>) {
        *self
            .getting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = part;
    }

    fn getting(&self) -> Option<u8> {
        *self
            .getting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) async fn get(&self) -> Result<(), Text> {
        if on_disk().is_some() {
            return Ok(());
        }
        let program = self.newest().await?;
        if let Some(browsers) = self.browsers.get() {
            browsers.use_engine(program);
        }
        Ok(())
    }

    fn keep_current(&self) {
        let host = self.clone();
        tokio::spawn(async move {
            loop {
                match host.newest().await {
                    Ok(program) => {
                        if let Some(browsers) = host.browsers.get() {
                            browsers.use_engine(program);
                        }
                    }
                    Err(problem) => tracing::debug!(%problem, "no newer browser now"),
                }
                tokio::time::sleep(LOOK_EVERY).await;
            }
        });
    }

    fn engines(&self) -> PathBuf {
        self.folder.join("engine")
    }

    fn profiles(&self) -> PathBuf {
        self.folder.join("profiles")
    }

    pub(super) async fn remove_profile(&self, profile: &str) -> Result<(), Text> {
        if let Some(browsers) = self.browsers.get() {
            return browsers
                .remove_profile(profile)
                .await
                .map_err(|problem| Text::plain(problem.to_string()));
        }
        match tokio::fs::remove_dir_all(self.profiles().join(profile)).await {
            Err(problem) if problem.kind() != std::io::ErrorKind::NotFound => {
                Err(Text::plain(problem.to_string()))
            }
            _ => Ok(()),
        }
    }

    pub(crate) async fn status(&self) -> BrowserStatus {
        let engine = match on_disk() {
            Some(program) => Some(program.display().to_string()),
            None => installed(&self.folder.join("engine"))
                .await
                .map(|(version, _)| format!("Chromium {version}")),
        };
        BrowserStatus {
            sources: shared_browser::sign_in_sources(),
            sites: shared_browser::copied_sites(&self.profiles(), KEPT_PROFILE),
            engine,
            getting: self.getting(),
        }
    }

    /// Read cookies from the source browser before initializing Kodosi Browser so permission
    /// failures do not trigger an unnecessary engine download.
    pub(crate) async fn copy_sign_ins(&self, name: &str) -> Result<usize, Text> {
        let sources = shared_browser::sign_in_sources();
        let source = sources
            .iter()
            .find(|source| source.name == name)
            .ok_or_else(|| text!("{name} is not on this computer.", name = name))?;
        let sign_ins = source
            .read(&self.folder.join(READING))
            .await
            .map_err(copy_problem)?;
        self.browsers(|_| {})
            .await?
            .add_sign_ins(KEPT_PROFILE, sign_ins)
            .await
            .map_err(copy_problem)
    }

    pub(crate) async fn forget_sign_ins(&self) -> Result<usize, Text> {
        if shared_browser::copied_sites(&self.profiles(), KEPT_PROFILE) == 0 {
            return Ok(0);
        }
        self.browsers(|_| {})
            .await?
            .remove_copied_sign_ins(KEPT_PROFILE)
            .await
            .map_err(|problem| Text::plain(problem.to_string()))
    }

    pub(crate) async fn clear(&self) -> Result<(), Text> {
        self.remove_profile(KEPT_PROFILE).await
    }

    pub(crate) async fn close(&self) {
        if let Some(browsers) = self.browsers.get() {
            browsers.close().await;
        }
    }
}

fn on_disk() -> Option<PathBuf> {
    std::env::var_os(ENGINE_PROGRAM_VARIABLE).map(PathBuf::from)
}

fn copy_problem(problem: shared_browser::CopyError) -> Text {
    use shared_browser::CopyError;
    match problem {
        CopyError::NoAccess(name) if cfg!(target_os = "macos") => text!(
            "macOS did not give Kodosi the key of the sign-ins of {name}. Copy again, and allow Kodosi when macOS asks.",
            name = name
        ),
        CopyError::NoAccess(name) => text!(
            "{name} did not give Kodosi the key of its sign-ins.",
            name = name
        ),
        CopyError::Locked(name) => text!(
            "{name} keeps its sign-ins locked on this computer, so sign in on the canvas.",
            name = name
        ),
        CopyError::Protected(name) => text!(
            "macOS lets only apps with Full Disk Access read the sign-ins of {name}.",
            name = name
        ),
        CopyError::Store(error) => text!("The sign-ins could not be read: {error}", error = error),
        CopyError::Browser(error) => Text::plain(error.to_string()),
    }
}

fn platform() -> String {
    let system = match std::env::consts::OS {
        "macos" => "mac",
        other => other,
    };
    let machine = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    };
    format!("{system}-{machine}")
}

/// Suffix with `.noindex` so macOS Spotlight skips the bundled Chromium app.
fn folder_of(version: &str) -> String {
    format!("{version}.noindex")
}

async fn installed(engines: &Path) -> Option<(String, PathBuf)> {
    let text = tokio::fs::read(engines.join(CURRENT)).await.ok()?;
    let current: Current = serde_json::from_slice(&text).ok()?;
    let program = engines
        .join(folder_of(&current.version))
        .join(&current.program);
    (current.link == LINK && program.exists()).then_some((current.version, program))
}

async fn install(engines: &Path, progress: impl Fn(u8) + Send) -> Result<PathBuf, Text> {
    let client = reqwest::Client::builder()
        .connect_timeout(LOOK_LIMIT)
        .read_timeout(LOOK_LIMIT)
        .build()
        .map_err(|_| no_engine())?;
    let feed = feed(&client).await?;
    if let Some(program) = kept(engines, &feed.version).await {
        return Ok(program);
    }
    let engine = feed
        .engines
        .get(&platform())
        .ok_or_else(not_for_this_computer)?;
    let folder = engines.join(folder_of(&feed.version));
    let program = folder.join(&engine.program);
    tokio::fs::create_dir_all(engines)
        .await
        .map_err(|_| no_engine())?;
    let archive = engines.join(format!("{}.tar.gz", feed.version));
    let got = download(&client, engine, &archive, progress).await;
    let unpacked = match got {
        Ok(()) => unpack(archive.clone(), folder.clone()).await,
        Err(problem) => Err(problem),
    };
    drop(tokio::fs::remove_file(&archive).await);
    unpacked?;
    let current = Current {
        version: feed.version.clone(),
        program: engine.program.clone(),
        link: LINK,
    };
    let text = serde_json::to_vec(&current).map_err(|_| no_engine())?;
    tokio::fs::write(engines.join(CURRENT), text)
        .await
        .map_err(|_| no_engine())?;
    Ok(program)
}

/// Reject downgrades to older versions: a valid signature proves authenticity, not freshness,
/// so whoever controls the update site could otherwise replay an older feed with known
/// vulnerabilities.
async fn kept(engines: &Path, offered: &str) -> Option<PathBuf> {
    let (version, program) = installed(engines).await?;
    (!newer(offered, &version)).then_some(program)
}

fn newer(offered: &str, installed: &str) -> bool {
    order(offered) > order(installed)
}

/// Parses `<chromium>-<kodosi_browser>` with the Kodosi Browser version first, so a newer
/// Kodosi Browser release wins even if it ships an earlier Chromium version.
fn order(version: &str) -> Option<(Vec<u64>, Vec<u64>)> {
    let (chromium, browser) = version.split_once('-')?;
    let fields = |part: &str| {
        part.split('.')
            .map(|field| field.parse().ok())
            .collect::<Option<Vec<u64>>>()
    };
    Some((fields(browser)?, fields(chromium)?))
}

async fn feed(client: &reqwest::Client) -> Result<Feed, Text> {
    let address = format!("{FEEDS}/{LINK}/engine.json");
    let bytes = get(client, &address).await?;
    let signature = get(client, &format!("{address}.sig")).await?;
    let text = std::str::from_utf8(&signature).map_err(|_| no_engine())?;
    check_signature(&bytes, text.trim())?;
    read_feed(&bytes)
}

/// Verify `feed.link` inside the signed payload because the signature does not cover the URL.
fn read_feed(bytes: &[u8]) -> Result<Feed, Text> {
    let feed: Feed = serde_json::from_slice(bytes).map_err(|_| no_engine())?;
    if feed.link == LINK {
        Ok(feed)
    } else {
        Err(not_for_this_computer())
    }
}

/// Treat HTTP 404 as "no browser published yet" rather than a transient network error.
async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, Text> {
    let response = client
        .get(url)
        .timeout(LOOK_LIMIT)
        .send()
        .await
        .map_err(|_| no_engine())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(not_for_this_computer());
    }
    let bytes = response
        .error_for_status()
        .map_err(|_| no_engine())?
        .bytes()
        .await
        .map_err(|_| no_engine())?;
    if bytes.len() > MOST_FEED {
        return Err(no_engine());
    }
    Ok(bytes.to_vec())
}

fn check_signature(feed: &[u8], signature: &str) -> Result<(), Text> {
    let decode = |text: &str| base64::engine::general_purpose::STANDARD.decode(text);
    let key = decode(FEED_KEY).map_err(|_| no_engine())?;
    let signature = decode(signature).map_err(|_| no_engine())?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(feed, &signature)
        .map_err(|_| no_engine())
}

async fn download(
    client: &reqwest::Client,
    engine: &Engine,
    to: &Path,
    progress: impl Fn(u8) + Send,
) -> Result<(), Text> {
    let mut response = client
        .get(&engine.url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| no_engine())?;
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(to).await.map_err(|_| no_engine())?;
    let mut digest = Context::new(&SHA256);
    let mut got: u64 = 0;
    let mut told = 0;
    while let Some(part) = response.chunk().await.map_err(|_| no_engine())? {
        got += part.len() as u64;
        if got > MOST_ARCHIVE {
            return Err(no_engine());
        }
        digest.update(&part);
        file.write_all(&part).await.map_err(|_| no_engine())?;
        let part_done = u8::try_from(got * 100 / total.max(1)).unwrap_or(100);
        if part_done != told {
            told = part_done;
            progress(told);
        }
    }
    file.flush().await.map_err(|_| no_engine())?;
    let found = crate::hex::hex(digest.finish().as_ref());
    (found == engine.sha256).ok_or_else(no_engine)
}

async fn unpack(archive: PathBuf, folder: PathBuf) -> Result<(), Text> {
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let unpacking = folder.with_extension("unpacking.noindex");
        drop(std::fs::remove_dir_all(&unpacking));
        std::fs::create_dir_all(&unpacking)?;
        let file = std::fs::File::open(&archive)?;
        tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(&unpacking)?;
        // Remove any existing dir with the same version name left by a different `LINK` version.
        drop(std::fs::remove_dir_all(&folder));
        std::fs::rename(&unpacking, &folder)
    })
    .await
    .map_err(|_| no_engine())?
    .map_err(|_| no_engine())
}

async fn remove_others(engines: &Path, version: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(engines).await else {
        return;
    };
    let current = folder_of(version);
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        if name == *current || name == CURRENT {
            continue;
        }
        if entry.path().is_dir() {
            drop(tokio::fs::remove_dir_all(entry.path()).await);
        } else {
            drop(tokio::fs::remove_file(entry.path()).await);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feed_with_a_wrong_signature_is_refused() {
        let wrong = base64::engine::general_purpose::STANDARD.encode([7_u8; 64]);
        assert!(check_signature(br#"{"version":"1"}"#, &wrong).is_err());
    }

    #[tokio::test]
    async fn an_engine_counts_only_when_its_program_is_there() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(installed(folder.path()).await, None);
        let current = |link: u32| {
            serde_json::json!({ "version": "1.2", "program": "bin/chrome", "link": link })
                .to_string()
        };
        std::fs::write(folder.path().join(CURRENT), current(LINK)).unwrap();
        assert_eq!(installed(folder.path()).await, None);
        std::fs::create_dir_all(folder.path().join("1.2/bin")).unwrap();
        std::fs::write(folder.path().join("1.2/bin/chrome"), "").unwrap();
        assert_eq!(installed(folder.path()).await, None);
        std::fs::create_dir_all(folder.path().join("1.2.noindex/bin")).unwrap();
        std::fs::write(folder.path().join("1.2.noindex/bin/chrome"), "").unwrap();
        assert_eq!(
            installed(folder.path()).await,
            Some((
                "1.2".to_owned(),
                folder.path().join("1.2.noindex/bin/chrome")
            ))
        );
        std::fs::write(folder.path().join(CURRENT), current(LINK + 1)).unwrap();
        assert_eq!(installed(folder.path()).await, None);
    }

    #[test]
    fn a_browser_with_a_higher_number_of_its_own_is_newer_on_any_chromium() {
        let installed = "156.0.8078.96-0.1.3";
        assert!(newer("156.0.8078.80-0.1.4", installed));
        assert!(newer("157.0.7000.1-0.1.3", installed));
        assert!(newer("156.0.8078.96-0.1.10", "156.0.8078.96-0.1.9"));
        assert!(!newer(installed, installed));
        assert!(!newer("157.0.7000.1-0.1.2", installed));
        assert!(!newer("156.0.8078.80-0.1.3", installed));
        assert!(!newer("157.0.7000.1", installed));
        assert!(!newer("157.0.x.1-0.1.4", installed));
        assert!(newer(installed, "1.2"));
    }

    #[tokio::test]
    async fn an_earlier_or_equal_feed_keeps_the_installed_engine() {
        let folder = tempfile::tempdir().unwrap();
        let version = "156.0.8078.96-0.1.3";
        let earlier = "156.0.8078.80-0.1.2";
        assert_eq!(kept(folder.path(), earlier).await, None);
        let program = folder.path().join(folder_of(version)).join("bin/chrome");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, "").unwrap();
        let current = |link: u32| {
            serde_json::json!({ "version": version, "program": "bin/chrome", "link": link })
                .to_string()
        };
        std::fs::write(folder.path().join(CURRENT), current(LINK)).unwrap();
        for offered in [earlier, "157.0.7000.1-0.1.2", version] {
            assert_eq!(
                kept(folder.path(), offered).await,
                Some(program.clone()),
                "{offered}"
            );
        }
        assert_eq!(kept(folder.path(), "156.0.8078.80-0.1.4").await, None);
        std::fs::write(folder.path().join(CURRENT), current(LINK + 1)).unwrap();
        assert_eq!(kept(folder.path(), earlier).await, None);
    }

    #[test]
    fn a_feed_of_another_version_of_the_link_gives_no_browser() {
        let feed = |link: u32| {
            serde_json::json!({ "version": "1.2", "link": link, "engines": {} }).to_string()
        };
        assert_eq!(
            read_feed(feed(LINK).as_bytes()).map(|feed| feed.version),
            Ok("1.2".to_owned())
        );
        assert_eq!(
            read_feed(feed(LINK + 1).as_bytes()).map(|feed| feed.version),
            Err(not_for_this_computer())
        );
    }

    #[tokio::test]
    async fn a_new_engine_takes_the_place_of_one_with_its_number_from_another_link() {
        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("1.2.tar.gz");
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            std::fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "bin/chrome", &b"new"[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let engine = folder.path().join(folder_of("1.2"));
        std::fs::create_dir_all(engine.join("bin")).unwrap();
        std::fs::write(engine.join("bin/chrome"), "old").unwrap();
        std::fs::write(engine.join("left"), "").unwrap();
        unpack(archive, engine.clone()).await.unwrap();
        assert_eq!(std::fs::read(engine.join("bin/chrome")).unwrap(), b"new");
        assert!(!engine.join("left").exists());
    }

    #[tokio::test]
    async fn a_start_keeps_only_the_current_engine() {
        let folder = tempfile::tempdir().unwrap();
        for engine in [
            "1.1.noindex/bin",
            "1.2/bin",
            "1.2.noindex/bin",
            "1.3.unpacking.noindex/bin",
        ] {
            std::fs::create_dir_all(folder.path().join(engine)).unwrap();
        }
        std::fs::write(folder.path().join("1.3.tar.gz"), "half").unwrap();
        std::fs::write(folder.path().join(CURRENT), "{}").unwrap();
        remove_others(folder.path(), "1.2").await;
        let mut left: Vec<String> = std::fs::read_dir(folder.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["1.2.noindex", CURRENT]);
    }
}
