use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::Engine as _;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use shared_browser::{Browsers, Settings};
use tokio::{io::AsyncWriteExt as _, sync::OnceCell};

/// Names a browser program on disk, for a build with no feed.
const ON_DISK: &str = "KODOSI_BROWSER_ENGINE";
const FEED: &str = "https://updates.kodosi.com/browser/engine.json";
/// The key of Kodosi's updates, which also signs this feed.
const FEED_KEY: &str = "6eh21KvaRNcq4kfPMxINLLm+OsMXLgIT6VWfYlU3fow=";
const MOST_FEED: usize = 64 * 1024;
const MOST_ARCHIVE: u64 = 1024 * 1024 * 1024;
const LOOK_LIMIT: Duration = Duration::from_secs(30);
const NO_ENGINE: &str = "Kodosi could not get its browser. Check the network and open it again.";
const NOT_FOR_THIS_COMPUTER: &str = "Kodosi has no browser for this computer yet.";
const CURRENT: &str = "current.json";

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

/// The host's browsers. Kodosi Browser comes the first time that a window opens on this
/// computer.
#[derive(Clone)]
pub(crate) struct BrowserHost {
    folder: PathBuf,
    browsers: Arc<OnceCell<Browsers>>,
}

impl BrowserHost {
    pub(crate) fn new(data_root: &Path) -> Self {
        Self {
            folder: data_root.join("browser"),
            browsers: Arc::default(),
        }
    }

    /// The browsers. `progress` hears the part of the first download, from 0 to 100.
    pub(super) async fn browsers(&self, progress: impl Fn(u8) + Send) -> Result<&Browsers, String> {
        self.browsers
            .get_or_try_init(|| async {
                let engines = self.folder.join("engine");
                let engine = match on_disk() {
                    Some(program) => program,
                    None => match installed(&engines).await {
                        Some(program) => {
                            let later = engines.clone();
                            tokio::spawn(async move { drop(install(&later, |_| {}).await) });
                            program
                        }
                        None => install(&engines, progress).await?,
                    },
                };
                Ok(Browsers::new(Settings {
                    engine,
                    profiles: self.folder.join("profiles"),
                }))
            })
            .await
    }

    pub(super) async fn remove_profile(&self, profile: &str) {
        match self.browsers.get() {
            Some(browsers) => {
                if let Err(problem) = browsers.remove_profile(profile).await {
                    tracing::warn!(%problem, "the profile of a closed terminal stays");
                }
            }
            None => {
                let folder = self.folder.join("profiles").join(profile);
                drop(tokio::fs::remove_dir_all(folder).await);
            }
        }
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

async fn installed(engines: &Path) -> Option<PathBuf> {
    let text = tokio::fs::read(engines.join(CURRENT)).await.ok()?;
    let current: Current = serde_json::from_slice(&text).ok()?;
    let program = engines.join(&current.version).join(&current.program);
    program.exists().then_some(program)
}

/// Gets the newest engine of the signed feed, unless it is here, and returns its program.
async fn install(engines: &Path, progress: impl Fn(u8) + Send) -> Result<PathBuf, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(LOOK_LIMIT)
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
    remove_others(engines, &feed.version).await;
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

async fn remove_others(engines: &Path, version: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(engines).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.path().is_dir() && entry.file_name() != version {
            drop(tokio::fs::remove_dir_all(entry.path()).await);
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
            Some(folder.path().join("1.2/bin/chrome"))
        );
    }
}
