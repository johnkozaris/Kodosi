use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Once},
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
const NO_ENGINE: &str = "Kodosi could not get its browser. Check the network and open it again.";
const NOT_FOR_THIS_COMPUTER: &str = "Kodosi has no browser for this computer yet.";
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
    pub(crate) sources: Vec<String>,
    pub(crate) sites: usize,
    pub(crate) engine: Option<String>,
}

/// The host's browsers. Kodosi Browser comes the first time that a window opens on this
/// computer.
#[derive(Clone)]
pub(crate) struct BrowserHost {
    folder: PathBuf,
    browsers: Arc<OnceCell<Browsers>>,
    updating: Arc<Once>,
}

impl BrowserHost {
    pub(crate) fn new(data_root: &Path) -> Self {
        Self {
            folder: data_root.join("browser"),
            browsers: Arc::default(),
            updating: Arc::new(Once::new()),
        }
    }

    /// The browsers. `progress` hears the part of the first download, from 0 to 100.
    pub(super) async fn browsers(&self, progress: impl Fn(u8) + Send) -> Result<&Browsers, String> {
        let browsers = self
            .browsers
            .get_or_try_init(|| async {
                let engine = match on_disk() {
                    Some(program) => program,
                    None => match installed(&self.engines()).await {
                        Some((version, program)) => {
                            remove_others(&self.engines(), &version).await;
                            program
                        }
                        None => install(&self.engines(), progress).await?,
                    },
                };
                Ok::<_, String>(Browsers::new(Settings {
                    engine,
                    profiles: self.profiles(),
                }))
            })
            .await?;
        if on_disk().is_none() {
            self.updating.call_once(|| self.keep_current());
        }
        Ok(browsers)
    }

    /// Gets each newer engine while the host runs. A browser that starts later uses it, a
    /// browser that runs keeps its own, and the older engine goes when the host starts again.
    fn keep_current(&self) {
        let engines = self.engines();
        let browsers = Arc::clone(&self.browsers);
        tokio::spawn(async move {
            loop {
                match install(&engines, |_| {}).await {
                    Ok(program) => {
                        if let Some(browsers) = browsers.get() {
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
    pub(super) async fn remove_profile(&self, profile: &str) -> Result<(), String> {
        if let Some(browsers) = self.browsers.get() {
            return browsers
                .remove_profile(profile)
                .await
                .map_err(|problem| problem.to_string());
        }
        match tokio::fs::remove_dir_all(self.profiles().join(profile)).await {
            Err(problem) if problem.kind() != std::io::ErrorKind::NotFound => {
                Err(problem.to_string())
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
            sources: shared_browser::sign_in_sources()
                .into_iter()
                .map(|source| source.name)
                .collect(),
            sites: shared_browser::copied_sites(&self.profiles(), KEPT_PROFILE),
            engine,
        }
    }

    /// Copies the sign-ins of a browser of this computer into the kept profile, and returns for
    /// how many sites. It reads them first, so a browser that gives no access costs no download.
    /// Only a person starts this.
    pub(crate) async fn copy_sign_ins(&self, name: &str) -> Result<usize, String> {
        let sources = shared_browser::sign_in_sources();
        let source = sources
            .iter()
            .find(|source| source.name == name)
            .ok_or_else(|| format!("{name} is not on this computer."))?;
        let sign_ins = source
            .read(&self.folder.join(READING))
            .await
            .map_err(|problem| problem.to_string())?;
        self.browsers(|_| {})
            .await?
            .add_sign_ins(KEPT_PROFILE, sign_ins)
            .await
            .map_err(|problem| problem.to_string())
    }

    /// Removes what copies gave the kept profile, and returns for how many sites.
    pub(crate) async fn forget_sign_ins(&self) -> Result<usize, String> {
        if shared_browser::copied_sites(&self.profiles(), KEPT_PROFILE) == 0 {
            return Ok(0);
        }
        self.browsers(|_| {})
            .await?
            .remove_copied_sign_ins(KEPT_PROFILE)
            .await
            .map_err(|problem| problem.to_string())
    }

    /// Removes the kept profile with every sign-in and all site data. The browsers of the
    /// private terminals close, and they wake with a new profile.
    pub(crate) async fn clear(&self) -> Result<(), String> {
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

/// The version and the program of the engine on this computer.
async fn installed(engines: &Path) -> Option<(String, PathBuf)> {
    let text = tokio::fs::read(engines.join(CURRENT)).await.ok()?;
    let current: Current = serde_json::from_slice(&text).ok()?;
    let program = engines.join(&current.version).join(&current.program);
    program.exists().then_some((current.version, program))
}

/// Gets the newest engine of the signed feed, unless it is here, and returns its program.
async fn install(engines: &Path, progress: impl Fn(u8) + Send) -> Result<PathBuf, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(LOOK_LIMIT)
        .read_timeout(LOOK_LIMIT)
        .build()
        .map_err(|_| NO_ENGINE.to_owned())?;
    let feed = feed(&client).await?;
    let engine = feed
        .engines
        .get(&platform())
        .ok_or_else(|| NOT_FOR_THIS_COMPUTER.to_owned())?;
    let folder = engines.join(&feed.version);
    let program = folder.join(&engine.program);
    if program.exists() {
        return Ok(program);
    }
    tokio::fs::create_dir_all(engines)
        .await
        .map_err(|_| NO_ENGINE.to_owned())?;
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
    let text = serde_json::to_vec(&current).map_err(|_| NO_ENGINE.to_owned())?;
    tokio::fs::write(engines.join(CURRENT), text)
        .await
        .map_err(|_| NO_ENGINE.to_owned())?;
    Ok(program)
}

async fn feed(client: &reqwest::Client) -> Result<Feed, String> {
    let bytes = get(client, FEED).await?;
    let signature = get(client, &format!("{FEED}.sig")).await?;
    let text = std::str::from_utf8(&signature).map_err(|_| NO_ENGINE.to_owned())?;
    check_signature(&bytes, text.trim())?;
    serde_json::from_slice(&bytes).map_err(|_| NO_ENGINE.to_owned())
}

async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let bytes = client
        .get(url)
        .timeout(LOOK_LIMIT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| NO_ENGINE.to_owned())?
        .bytes()
        .await
        .map_err(|_| NO_ENGINE.to_owned())?;
    if bytes.len() > MOST_FEED {
        return Err(NO_ENGINE.to_owned());
    }
    Ok(bytes.to_vec())
}

fn check_signature(feed: &[u8], signature: &str) -> Result<(), String> {
    let decode = |text: &str| base64::engine::general_purpose::STANDARD.decode(text);
    let key = decode(FEED_KEY).map_err(|_| NO_ENGINE.to_owned())?;
    let signature = decode(signature).map_err(|_| NO_ENGINE.to_owned())?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(feed, &signature)
        .map_err(|_| NO_ENGINE.to_owned())
}

async fn download(
    client: &reqwest::Client,
    engine: &Engine,
    to: &Path,
    progress: impl Fn(u8) + Send,
) -> Result<(), String> {
    let response = client
        .get(&engine.url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| NO_ENGINE.to_owned())?;
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(to)
        .await
        .map_err(|_| NO_ENGINE.to_owned())?;
    let mut digest = Sha256::new();
    let mut got: u64 = 0;
    let mut told = 0;
    let mut stream = response.bytes_stream();
    while let Some(part) = stream.next().await {
        let part = part.map_err(|_| NO_ENGINE.to_owned())?;
        got += part.len() as u64;
        if got > MOST_ARCHIVE {
            return Err(NO_ENGINE.to_owned());
        }
        digest.update(&part);
        file.write_all(&part)
            .await
            .map_err(|_| NO_ENGINE.to_owned())?;
        let part_done = u8::try_from(got * 100 / total.max(1)).unwrap_or(100);
        if part_done != told {
            told = part_done;
            progress(told);
        }
    }
    file.flush().await.map_err(|_| NO_ENGINE.to_owned())?;
    let found = digest
        .finalize()
        .iter()
        .fold(String::new(), |text, byte| text + &format!("{byte:02x}"));
    if found == engine.sha256 {
        Ok(())
    } else {
        Err(NO_ENGINE.to_owned())
    }
}

/// Unpacks into a new folder, which takes its place only when it is whole.
async fn unpack(archive: PathBuf, folder: PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let unpacking = folder.with_extension("unpacking");
        drop(std::fs::remove_dir_all(&unpacking));
        std::fs::create_dir_all(&unpacking)?;
        let file = std::fs::File::open(&archive)?;
        tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(&unpacking)?;
        std::fs::rename(&unpacking, &folder)
    })
    .await
    .map_err(|_| NO_ENGINE.to_owned())?
    .map_err(|_| NO_ENGINE.to_owned())
}

/// Removes each engine but this version, with the archives of downloads that stopped.
async fn remove_others(engines: &Path, version: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(engines).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        if name == version || name == CURRENT {
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
        assert_eq!(
            installed(folder.path()).await,
            Some(("1.2".to_owned(), folder.path().join("1.2/bin/chrome")))
        );
    }

    #[tokio::test]
    async fn a_start_keeps_only_the_current_engine() {
        let folder = tempfile::tempdir().unwrap();
        for engine in ["1.1/bin", "1.2/bin"] {
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
        assert_eq!(left, ["1.2", CURRENT]);
    }
}
