use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Once},
    time::Duration,
};

use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::Engine as _;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use shared_browser::{Browsers, Settings};
use tokio::{io::AsyncWriteExt as _, sync::OnceCell};

use super::browser::KEPT_PROFILE;
use crate::text::Text;

/// Names a browser program on disk, for a build with no feed.
const ON_DISK: &str = "KODOSI_BROWSER_ENGINE";
const FEED: &str = "https://updates.kodosi.com/browser/engine.json";
/// The key of Kodosi's updates, which also signs this feed.
const FEED_KEY: &str = "6eh21KvaRNcq4kfPMxINLLm+OsMXLgIT6VWfYlU3fow=";
const MOST_FEED: usize = 64 * 1024;
const MOST_ARCHIVE: u64 = 1024 * 1024 * 1024;
const LOOK_LIMIT: Duration = Duration::from_secs(30);
/// Chromium has a security release about every two weeks, so a host looks each day.
const LOOK_EVERY: Duration = Duration::from_hours(24);
fn no_engine() -> Text {
    text!("Kodosi could not get its browser. Check the network and open it again.")
}

fn not_for_this_computer() -> Text {
    text!("Kodosi has no browser for this computer yet.")
}

const CURRENT: &str = "current.json";
/// A private folder for the copies of the stores that a copy of sign-ins reads.
const READING: &str = "sign-in-copy";

#[derive(Debug, Deserialize)]
struct Feed {
    version: String,
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
}

/// What Settings › Browser shows.
pub(crate) struct BrowserStatus {
    pub(crate) sources: Vec<shared_browser::SignInSource>,
    pub(crate) sites: usize,
    pub(crate) engine: Option<String>,
    /// The part of a download of Kodosi Browser, from 0 to 100, while one runs.
    pub(crate) getting: Option<u8>,
}

/// The host's browsers. Kodosi Browser comes the first time that a window opens on this
/// computer, or when a person gets it in Settings.
#[derive(Clone)]
pub(crate) struct BrowserHost {
    folder: PathBuf,
    /// The data root is isolated, so the browsers must not touch the person's keychain.
    isolated: bool,
    browsers: Arc<OnceCell<Browsers>>,
    updating: Arc<Once>,
    /// One download at a time, so no two write the same engine and no start removes one that
    /// comes.
    installing: Arc<tokio::sync::Mutex<()>>,
    getting: Arc<Mutex<Option<u8>>>,
}

impl BrowserHost {
    pub(crate) fn new(data_root: &Path, isolated: bool) -> Self {
        Self {
            folder: data_root.join("browser"),
            isolated,
            browsers: Arc::default(),
            updating: Arc::new(Once::new()),
            installing: Arc::default(),
            getting: Arc::default(),
        }
    }

    /// The browsers. `progress` hears the part of the first download, from 0 to 100.
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

    /// The engine on this computer, with the others removed, or else the newest of the feed.
    async fn engine(&self, progress: impl Fn(u8) + Send) -> Result<PathBuf, Text> {
        let _one = self.installing.lock().await;
        if let Some((version, program)) = installed(&self.engines()).await {
            remove_others(&self.engines(), &version).await;
            return Ok(program);
        }
        self.download(progress).await
    }

    /// Gets the newest engine of the feed unless it is here. The caller holds `installing`.
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

    /// Holds back each download of Kodosi Browser while the guard lives; none when the engine is
    /// on disk.
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

    /// A person's own step in Settings: gets Kodosi Browser now, or its newest version. A
    /// browser that runs keeps its own engine, as with each daily look.
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

    /// Gets each newer engine while the host runs. A browser that starts later uses it, a
    /// browser that runs keeps its own, and the older engine goes when the host starts again.
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

    /// Closes the browser of a profile and deletes the profile.
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

    /// What Settings › Browser shows: the browsers to copy from, for how many sites copies gave
    /// the kept profile sign-ins, and the Kodosi Browser of this computer.
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

    /// Copies the sign-ins of a browser of this computer into the kept profile, and returns for
    /// how many sites. It reads them first, so a browser that gives no access costs no download.
    /// Only a person starts this.
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

    /// Removes what copies gave the kept profile, and returns for how many sites.
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

    /// Removes the kept profile with every sign-in and all site data. The browsers of the
    /// private terminals close, and they wake with a new profile.
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
    std::env::var_os(ON_DISK).map(PathBuf::from)
}

/// Why sign-ins did not copy, in the words of the person's language.
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

/// Spotlight skips a `.noindex` folder, so macOS does not show the engine as a second app.
fn folder_of(version: &str) -> String {
    format!("{version}.noindex")
}

/// The version and the program of the engine on this computer.
async fn installed(engines: &Path) -> Option<(String, PathBuf)> {
    let text = tokio::fs::read(engines.join(CURRENT)).await.ok()?;
    let current: Current = serde_json::from_slice(&text).ok()?;
    let program = engines
        .join(folder_of(&current.version))
        .join(&current.program);
    program.exists().then_some((current.version, program))
}

/// Gets the newest engine of the signed feed, unless it is here, and returns its program.
async fn install(engines: &Path, progress: impl Fn(u8) + Send) -> Result<PathBuf, Text> {
    let client = reqwest::Client::builder()
        .connect_timeout(LOOK_LIMIT)
        .read_timeout(LOOK_LIMIT)
        .build()
        .map_err(|_| no_engine())?;
    let feed = feed(&client).await?;
    let engine = feed
        .engines
        .get(&platform())
        .ok_or_else(not_for_this_computer)?;
    let folder = engines.join(folder_of(&feed.version));
    let program = folder.join(&engine.program);
    if program.exists() {
        return Ok(program);
    }
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
    };
    let text = serde_json::to_vec(&current).map_err(|_| no_engine())?;
    tokio::fs::write(engines.join(CURRENT), text)
        .await
        .map_err(|_| no_engine())?;
    Ok(program)
}

async fn feed(client: &reqwest::Client) -> Result<Feed, Text> {
    let bytes = get(client, FEED).await?;
    let signature = get(client, &format!("{FEED}.sig")).await?;
    let text = std::str::from_utf8(&signature).map_err(|_| no_engine())?;
    check_signature(&bytes, text.trim())?;
    serde_json::from_slice(&bytes).map_err(|_| no_engine())
}

/// A feed that is not there means that no browser is out yet, which no network can change.
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
    let response = client
        .get(&engine.url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| no_engine())?;
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(to).await.map_err(|_| no_engine())?;
    let mut digest = Sha256::new();
    let mut got: u64 = 0;
    let mut told = 0;
    let mut stream = response.bytes_stream();
    while let Some(part) = stream.next().await {
        let part = part.map_err(|_| no_engine())?;
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
    let found = digest
        .finalize()
        .iter()
        .fold(String::new(), |text, byte| text + &format!("{byte:02x}"));
    if found == engine.sha256 {
        Ok(())
    } else {
        Err(no_engine())
    }
}

/// Unpacks into a new folder, which takes its place only when it is whole.
async fn unpack(archive: PathBuf, folder: PathBuf) -> Result<(), Text> {
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let unpacking = folder.with_extension("unpacking.noindex");
        drop(std::fs::remove_dir_all(&unpacking));
        std::fs::create_dir_all(&unpacking)?;
        let file = std::fs::File::open(&archive)?;
        tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(&unpacking)?;
        std::fs::rename(&unpacking, &folder)
    })
    .await
    .map_err(|_| no_engine())?
    .map_err(|_| no_engine())
}

/// Removes each engine but this version, with the archives of downloads that stopped.
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
    async fn a_feed_that_is_not_out_says_that_there_is_no_browser_yet() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            drop(tokio::io::AsyncReadExt::read(&mut stream, &mut request).await);
            let answer =
                b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut stream, answer)
                .await
                .unwrap();
        });
        let feed = get(
            &reqwest::Client::new(),
            &format!("http://{address}/engine.json"),
        )
        .await;
        assert_eq!(feed, Err(not_for_this_computer()));
    }

    #[test]
    fn each_computer_has_a_name_in_the_feed() {
        assert!(
            [
                "mac-arm64",
                "mac-x64",
                "linux-x64",
                "linux-arm64",
                "windows-x64"
            ]
            .contains(&platform().as_str())
        );
    }

    #[tokio::test]
    async fn an_engine_counts_only_when_its_program_is_there() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(installed(folder.path()).await, None);
        let current = serde_json::json!({ "version": "1.2", "program": "bin/chrome" });
        std::fs::write(folder.path().join(CURRENT), current.to_string()).unwrap();
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
