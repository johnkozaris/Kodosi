use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

use crate::{AppError, Result, support::platform::fs as support_fs};

#[path = "paths_unix.rs"]
mod imp;

const DATA_ROOT_ENV: &str = "KODOSI_DATA_ROOT";
const PRODUCTION_DATA_ROOT_ENV: &str = "KODOSI_PRODUCTION_DATA_ROOT";

pub(crate) fn validate_storage_contract() -> Result<()> {
    if let Some(root) = isolated_root()? {
        support_fs::ensure_dir(&root)?;
        let production = env::var_os(PRODUCTION_DATA_ROOT_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!(
                    "{PRODUCTION_DATA_ROOT_ENV} is required when {DATA_ROOT_ENV} is configured"
                ),
            })?;
        validate_isolated_root(&root, &production)?;
    }
    Ok(())
}

pub(crate) fn isolated_root() -> Result<Option<PathBuf>> {
    let Some(raw) = env::var_os(DATA_ROOT_ENV) else {
        return Ok(None);
    };
    let production = env::var_os(PRODUCTION_DATA_ROOT_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "{PRODUCTION_DATA_ROOT_ENV} is required when {DATA_ROOT_ENV} is configured"
            ),
        })?;
    validate_isolated_root(Path::new(&raw), &production).map(Some)
}

pub(crate) fn data_root() -> Result<PathBuf> {
    Ok(match isolated_root()? {
        Some(root) => root.join("core"),
        None => production_data_root()?,
    })
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn collaboration_teardown_obligations_path() -> Result<PathBuf> {
    Ok(data_root()?.join("collaboration-teardown-obligations.json"))
}

pub(crate) fn runtime_authority_lock_path() -> Result<PathBuf> {
    Ok(data_root()?.join("runtime-authority.lock"))
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn identity_reset_intent_path() -> Result<PathBuf> {
    Ok(data_root()?.join("identity-reset-intent.json"))
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn cache_root() -> Result<Option<PathBuf>> {
    isolated_root().map(|root| root.map(|path| path.join("cache")))
}

pub(crate) fn room_agent_root() -> Result<Option<PathBuf>> {
    isolated_root().map(|root| root.map(|path| path.join("agent-room")))
}

pub(crate) fn tokens_dir() -> Result<PathBuf> {
    isolated_root()?.map_or_else(imp::tokens_dir, |root| Ok(root.join("core")))
}

pub(crate) fn secrets_dir() -> Result<PathBuf> {
    isolated_root()?.map_or_else(imp::secrets_dir, |root| Ok(root.join("core")))
}

pub(crate) fn device_list_pins_path() -> Result<PathBuf> {
    Ok(data_root()?.join("device-list-pins.json"))
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn room_roster_pins_path() -> Result<PathBuf> {
    Ok(data_root()?.join("room-roster-pins.json"))
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn hidden_sessions_path() -> Result<PathBuf> {
    Ok(data_root()?.join("hidden-sessions.json"))
}

pub(crate) fn session_catalog_dir() -> Result<PathBuf> {
    Ok(data_root()?.join("session-catalog"))
}

fn production_data_root() -> Result<PathBuf> {
    imp::data_root()
}

fn validate_isolated_root(candidate: &Path, production_root: &Path) -> Result<PathBuf> {
    let display = candidate.display();
    if candidate.as_os_str().is_empty() {
        return invalid_root("must not be empty");
    }
    if !candidate.is_absolute() {
        return invalid_root(format!("must be absolute (got `{display}`)"));
    }
    if candidate
        .to_string_lossy()
        .contains(['$', '\n', '\r', '\0'])
    {
        return invalid_root("must not contain unresolved variables or control characters");
    }
    if candidate
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return invalid_root("must not contain `.` or `..` components");
    }

    let normalized = normalize_absolute(candidate)?;
    if normalized.parent().is_none() {
        return invalid_root("must not be the filesystem root");
    }

    if let Some(home) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        let home = normalize_absolute(&home)?;
        if normalized == home {
            return invalid_root("must not be the user home directory");
        }
    }

    let production = normalize_absolute(production_root)?;
    if normalized == production || normalized.starts_with(&production) {
        return invalid_root("must not be the production Kodosi data directory or a descendant");
    }

    reject_existing_symlinks(&normalized)?;
    if shares_existing_prefix_identity(&normalized, &production)? {
        return invalid_root("must not alias the production Kodosi data directory or a descendant");
    }
    Ok(normalized)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::Normal(value) => normalized.push(value),
            Component::CurDir | Component::ParentDir => {
                return invalid_root("must not contain `.` or `..` components");
            }
        }
    }
    Ok(normalized)
}

#[cfg(unix)]
fn shares_existing_prefix_identity(candidate: &Path, protected: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let protected = match fs::metadata(protected) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(AppError::Io(error)),
    };
    let mut current = PathBuf::new();
    for component in candidate.components() {
        current.push(component.as_os_str());
        match fs::metadata(&current) {
            Ok(metadata)
                if metadata.dev() == protected.dev() && metadata.ino() == protected.ino() =>
            {
                return Ok(true);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(AppError::Io(error)),
        }
    }
    Ok(false)
}

fn reject_existing_symlinks(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return invalid_root(format!("must not traverse symlink `{}`", current.display()));
            }
            Ok(metadata) if !metadata.is_dir() && current != path => {
                return invalid_root(format!(
                    "ancestor `{}` is not a directory",
                    current.display()
                ));
            }
            Ok(metadata) if current == path && !metadata.is_dir() => {
                return invalid_root("must identify a directory");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(AppError::Io(error)),
        }
    }
    Ok(())
}

fn invalid_root<T>(reason: impl Into<String>) -> Result<T> {
    Err(AppError::Unsupported {
        reason: format!("invalid {DATA_ROOT_ENV}: {}", reason.into()),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::validate_isolated_root;

    #[test]
    fn isolated_root_accepts_a_private_absolute_sibling() {
        let parent = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let candidate = parent
            .path()
            .canonicalize()
            .unwrap_or_else(|error| panic!("canonical tempdir: {error}"))
            .join("kodosi-isolated");
        let root = validate_isolated_root(
            &candidate,
            Path::new("/Users/test/Library/Application Support/kodosi"),
        )
        .unwrap_or_else(|error| panic!("root should be accepted: {error}"));
        assert_eq!(root, candidate);
    }

    #[test]
    fn isolated_root_rejects_unsafe_shapes() {
        let production = Path::new("/Users/test/Library/Application Support/kodosi");
        for candidate in [
            "",
            "relative",
            "/tmp/../tmp/kodosi",
            "/",
            "/tmp/$(BUILD_ROOT)",
            "/Users/test/Library/Application Support/kodosi",
            "/Users/test/Library/Application Support/kodosi/tests",
        ] {
            assert!(
                validate_isolated_root(Path::new(candidate), production).is_err(),
                "unsafe root should fail: {candidate}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn isolated_root_rejects_existing_identity_alias_and_missing_descendant() {
        let production = tempfile::tempdir().unwrap_or_else(|error| panic!("production: {error}"));
        let canonical = production
            .path()
            .canonicalize()
            .unwrap_or_else(|error| panic!("canonical production: {error}"));

        assert!(validate_isolated_root(&canonical, &canonical).is_err());
        assert!(validate_isolated_root(&canonical.join("missing"), &canonical).is_err());
        assert!(!canonical.join("missing").exists());
    }

    #[cfg(unix)]
    #[test]
    fn isolated_root_rejects_existing_symlink_components() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let target = tempfile::tempdir().unwrap_or_else(|error| panic!("target: {error}"));
        let link = parent.path().join("link");
        symlink(target.path(), &link).unwrap_or_else(|error| panic!("symlink: {error}"));

        assert!(
            validate_isolated_root(
                &link.join("isolated"),
                Path::new("/Users/test/Library/Application Support/kodosi")
            )
            .is_err()
        );
    }
}
