use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;

const DEFAULT_HANDOFF_CAPACITY: usize = 32;
const DEFAULT_HANDOFF_TTL: Duration = Duration::from_mins(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NativeOpenHandoff {
    pub handoff_id: String,
    pub handoff_path: String,
    pub display_name: String,
}

#[derive(Debug)]
pub struct NativeOpenHandoffRegistry {
    handoffs: HashMap<uuid::Uuid, HeldHandoff>,
    insertion_order: VecDeque<uuid::Uuid>,
    capacity: usize,
    ttl: Duration,
}

impl Default for NativeOpenHandoffRegistry {
    fn default() -> Self {
        Self {
            handoffs: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity: DEFAULT_HANDOFF_CAPACITY,
            ttl: DEFAULT_HANDOFF_TTL,
        }
    }
}

impl NativeOpenHandoffRegistry {
    pub fn prepare(
        &mut self,
        file: std::fs::File,
        display_name: String,
    ) -> Result<NativeOpenHandoff, String> {
        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect native open handoff: {error}"))?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err("native open handoff requires a regular file or directory".to_owned());
        }
        let platform = platform_handoff_path(&file, &display_name)?;
        let now = Instant::now();
        self.purge_expired(now);
        while self.handoffs.len() >= self.capacity {
            let Some(oldest) = self.insertion_order.pop_front() else {
                break;
            };
            self.handoffs.remove(&oldest);
        }
        let id = uuid::Uuid::now_v7();
        self.insertion_order.push_back(id);
        self.handoffs.insert(
            id,
            HeldHandoff {
                expires_at: now + self.ttl,
                _file: file,
                _artifact: platform.artifact,
            },
        );
        Ok(NativeOpenHandoff {
            handoff_id: id.to_string(),
            handoff_path: platform.path,
            display_name,
        })
    }

    pub fn release(&mut self, handoff_id: &str) -> Result<(), String> {
        let id = parse_id(handoff_id)?;
        self.handoffs
            .remove(&id)
            .ok_or_else(|| "native open handoff is stale".to_owned())?;
        self.insertion_order.retain(|candidate| *candidate != id);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.handoffs.clear();
        self.insertion_order.clear();
    }

    pub fn purge_expired_now(&mut self) {
        self.purge_expired(Instant::now());
    }

    fn purge_expired(&mut self, now: Instant) {
        self.handoffs.retain(|_, handoff| handoff.expires_at > now);
        self.insertion_order
            .retain(|id| self.handoffs.contains_key(id));
    }

    #[cfg(test)]
    fn with_limits(capacity: usize, ttl: Duration) -> Self {
        Self {
            handoffs: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity,
            ttl,
        }
    }
}

#[derive(Debug)]
struct HeldHandoff {
    expires_at: Instant,
    _artifact: Option<tempfile::TempDir>,
    _file: std::fs::File,
}

struct PlatformHandoff {
    path: String,
    artifact: Option<tempfile::TempDir>,
}

fn parse_id(value: &str) -> Result<uuid::Uuid, String> {
    let id =
        uuid::Uuid::parse_str(value).map_err(|_| "invalid native open handoff id".to_owned())?;
    if id.get_version_num() != 7 || id.hyphenated().to_string() != value {
        return Err("invalid native open handoff id".to_owned());
    }
    Ok(id)
}

#[cfg(target_os = "linux")]
fn platform_handoff_path(
    file: &std::fs::File,
    _display_name: &str,
) -> Result<PlatformHandoff, String> {
    use std::{
        os::fd::AsRawFd,
        os::unix::fs::{MetadataExt as _, symlink},
    };

    let artifact = private_artifact_directory()?;
    let artifact_path = artifact.path().join("handoff");
    let held_path = format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd());
    symlink(&held_path, &artifact_path)
        .map_err(|error| format!("bind Linux native open handoff artifact: {error}"))?;
    let published = std::fs::metadata(&artifact_path)
        .map_err(|error| format!("verify Linux native open handoff artifact: {error}"))?;
    let held = file
        .metadata()
        .map_err(|error| format!("inspect held Linux native open handoff: {error}"))?;
    if held.dev() != published.dev() || held.ino() != published.ino() {
        return Err("Linux native open handoff artifact does not name the held object".to_owned());
    }
    Ok(PlatformHandoff {
        path: artifact_path.to_string_lossy().into_owned(),
        artifact: Some(artifact),
    })
}

#[cfg(target_os = "macos")]
fn platform_handoff_path(
    file: &std::fs::File,
    display_name: &str,
) -> Result<PlatformHandoff, String> {
    use std::{
        ffi::CStr,
        os::{
            fd::AsRawFd,
            raw::{c_char, c_int},
            unix::fs::MetadataExt as _,
        },
    };

    const F_GETPATH: c_int = 50;
    const MAXPATHLEN: usize = 1024;
    unsafe extern "C" {
        fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    }

    let mut buffer = [0 as c_char; MAXPATHLEN];
    // SAFETY: F_GETPATH writes a NUL-terminated path into a MAXPATHLEN buffer.
    let result = unsafe { fcntl(file.as_raw_fd(), F_GETPATH, buffer.as_mut_ptr()) };
    if result == -1 {
        return Err(format!(
            "resolve macOS native open handoff path: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: a successful F_GETPATH call guarantees a NUL-terminated C string.
    let path = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .map_err(|error| format!("macOS native open handoff path is not UTF-8: {error}"))?
        .to_owned();
    if !std::path::Path::new(&path).is_absolute() {
        return Err("macOS native open handoff path is not absolute".to_owned());
    }
    let held = file
        .metadata()
        .map_err(|error| format!("inspect held macOS native open handoff: {error}"))?;
    let artifact = private_artifact_directory()?;
    let filename = std::path::Path::new(display_name)
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("handoff"));
    let artifact_path = artifact.path().join(filename);
    if held.is_file() {
        std::fs::hard_link(&path, &artifact_path)
            .map_err(|error| format!("bind macOS native open handoff artifact: {error}"))?;
    } else {
        use std::os::unix::fs::symlink;
        let volume_path = format!("/.vol/{}/{}", held.dev(), held.ino());
        symlink(&volume_path, &artifact_path)
            .map_err(|error| format!("bind macOS directory handoff artifact: {error}"))?;
    }
    let artifact_metadata = std::fs::File::open(&artifact_path)
        .and_then(|current| current.metadata())
        .map_err(|error| format!("verify macOS native open handoff artifact: {error}"))?;
    if held.dev() != artifact_metadata.dev() || held.ino() != artifact_metadata.ino() {
        return Err("macOS native open handoff artifact does not name the held file".to_owned());
    }
    Ok(PlatformHandoff {
        path: artifact_path.to_string_lossy().into_owned(),
        artifact: Some(artifact),
    })
}

fn private_artifact_directory() -> Result<tempfile::TempDir, String> {
    use std::os::unix::fs::PermissionsExt as _;

    let base = runtime_temp_base();
    let artifact = tempfile::Builder::new()
        .prefix("kodosi-open-")
        .tempdir_in(&base)
        .map_err(|error| {
            format!(
                "create native open handoff directory in {}: {error}",
                base.display()
            )
        })?;
    std::fs::set_permissions(artifact.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("secure native open handoff directory: {error}"))?;
    Ok(artifact)
}

fn runtime_temp_base() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && is_private_runtime_directory(path))
        .unwrap_or_else(std::env::temp_dir)
}

fn is_private_runtime_directory(path: &Path) -> bool {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == rustix::process::geteuid().as_raw()
        && metadata.permissions().mode().trailing_zeros() >= 6
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_handoff_path(
    _file: &std::fs::File,
    _display_name: &str,
) -> Result<PlatformHandoff, String> {
    Err("bound native open handoff is unavailable on this platform".to_owned())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn handoff_holds_exact_file_and_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.md");
        let replacement_path = directory.path().join("other.md");
        std::fs::write(&path, "memory").unwrap();
        std::fs::write(&replacement_path, "other").unwrap();
        let file = std::fs::File::open(path).unwrap();
        let mut registry = NativeOpenHandoffRegistry::with_limits(1, Duration::from_mins(1));
        let first = registry.prepare(file, "memory.md".to_owned()).unwrap();
        assert!(std::fs::read_to_string(&first.handoff_path).is_ok());
        let second = registry
            .prepare(
                std::fs::File::open(replacement_path).unwrap(),
                "other.md".to_owned(),
            )
            .unwrap();
        assert!(!Path::new(&first.handoff_path).exists());
        assert!(std::fs::metadata(&second.handoff_path).is_ok());
    }

    #[test]
    fn handoff_paths_and_ids_are_unique_while_retained() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.md");
        std::fs::write(&path, "memory").unwrap();
        let mut registry = NativeOpenHandoffRegistry::with_limits(2, Duration::from_mins(1));
        let first = registry
            .prepare(std::fs::File::open(&path).unwrap(), "memory.md".to_owned())
            .unwrap();
        let second = registry
            .prepare(std::fs::File::open(&path).unwrap(), "memory.md".to_owned())
            .unwrap();
        assert_ne!(first.handoff_id, second.handoff_id);
        assert_ne!(first.handoff_path, second.handoff_path);
        assert!(Path::new(&first.handoff_path).exists());
        assert!(Path::new(&second.handoff_path).exists());
    }

    #[cfg(all(test, target_os = "macos"))]
    mod macos_tests {
        use super::*;

        #[test]
        fn artifact_survives_asynchronous_handoff_until_explicit_release() {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("agent.md");
            let renamed = directory.path().join("renamed.md");
            std::fs::write(&source, "agent").unwrap();
            let mut registry = NativeOpenHandoffRegistry::default();
            let handoff = registry
                .prepare(std::fs::File::open(&source).unwrap(), "agent.md".to_owned())
                .unwrap();
            std::fs::rename(&source, &renamed).unwrap();
            std::thread::sleep(Duration::from_millis(25));
            assert_eq!(
                std::fs::read_to_string(&handoff.handoff_path).unwrap(),
                "agent"
            );
            registry.release(&handoff.handoff_id).unwrap();
            assert!(!Path::new(&handoff.handoff_path).exists());
        }

        #[test]
        fn directory_artifact_is_retained_until_release() {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("workspace");
            let renamed = directory.path().join("workspace-renamed");
            std::fs::create_dir(&source).unwrap();
            let mut registry = NativeOpenHandoffRegistry::default();
            let handoff = registry
                .prepare(
                    std::fs::File::open(&source).unwrap(),
                    "workspace".to_owned(),
                )
                .unwrap();
            std::fs::rename(&source, &renamed).unwrap();
            assert!(Path::new(&handoff.handoff_path).is_dir());
            registry.release(&handoff.handoff_id).unwrap();
            assert!(!Path::new(&handoff.handoff_path).exists());
        }
    }

    #[test]
    fn release_is_exact_and_one_shot() {
        use std::os::fd::AsRawFd as _;

        const CHILD_ENV: &str = "KODOSI_HANDOFF_FD_REUSE_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("ops::open_handoff::tests::release_is_exact_and_one_shot")
                .arg("--exact")
                .arg("--test-threads=1")
                .env(CHILD_ENV, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.md");
        std::fs::write(&path, "agent").unwrap();
        let mut registry = NativeOpenHandoffRegistry::default();
        let file = std::fs::File::open(path).unwrap();
        let released_fd = file.as_raw_fd();
        let handoff = registry.prepare(file, "agent.md".to_owned()).unwrap();
        let stale_path = handoff.handoff_path.clone();
        registry.release(&handoff.handoff_id).unwrap();
        assert!(registry.release(&handoff.handoff_id).is_err());
        assert!(!Path::new(&stale_path).exists());

        let mut reused = false;
        let mut held_files = Vec::new();
        for index in 0..256 {
            let replacement = directory.path().join(format!("replacement-{index}"));
            std::fs::write(&replacement, "replacement").unwrap();
            let held = std::fs::File::open(&replacement).unwrap();
            reused |= held.as_raw_fd() == released_fd;
            held_files.push(held);
            assert!(!Path::new(&stale_path).exists());
            if reused {
                break;
            }
        }
        assert!(reused, "released handoff descriptor was not reused");
        assert!(
            held_files
                .iter()
                .any(|file| file.as_raw_fd() == released_fd)
        );
    }

    #[test]
    fn expiry_and_clear_close_without_a_subsequent_prepare() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.md");
        std::fs::write(&path, "agent").unwrap();
        let mut registry = NativeOpenHandoffRegistry::with_limits(2, Duration::ZERO);
        let expired = registry
            .prepare(std::fs::File::open(&path).unwrap(), "agent.md".to_owned())
            .unwrap();
        registry.purge_expired_now();
        assert!(!Path::new(&expired.handoff_path).exists());

        let held = NativeOpenHandoffRegistry::with_limits(2, Duration::from_mins(1));
        let mut registry = held;
        let cleared = registry
            .prepare(std::fs::File::open(path).unwrap(), "agent.md".to_owned())
            .unwrap();
        registry.clear();
        assert!(!Path::new(&cleared.handoff_path).exists());
    }

    #[test]
    fn directory_handoff_is_private_and_revoked() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().unwrap();
        let file = std::fs::File::open(directory.path()).unwrap();
        let mut registry = NativeOpenHandoffRegistry::default();
        let handoff = registry.prepare(file, "directory".to_owned()).unwrap();
        let artifact = Path::new(&handoff.handoff_path);
        assert!(artifact.is_dir());
        let mode = artifact
            .parent()
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        registry.release(&handoff.handoff_id).unwrap();
        assert!(!artifact.exists());
    }
}
