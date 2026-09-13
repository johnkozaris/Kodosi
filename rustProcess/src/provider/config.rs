use std::path::{Path, PathBuf};

use rustix::fs::{Access, AtFlags, CWD, accessat};

use super::{ConfigFile, Provider, ProviderInfo};

pub(super) fn resolve_executable(provider: Provider) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .find_map(|directory| executable(&directory.join(provider.executable())))
    })
}

pub(super) fn executable(path: &Path) -> Option<PathBuf> {
    if !path.is_file() || accessat(CWD, path, Access::EXEC_OK, AtFlags::EACCESS).is_err() {
        return None;
    }
    path.canonicalize().ok()
}

pub(super) fn inspect(
    home: &Path,
    provider: Provider,
    directory: Option<&Path>,
) -> Result<ProviderInfo, String> {
    if !home.is_absolute() || !home.is_dir() {
        return Err("Provider home must be an existing absolute directory".to_owned());
    }
    let mut paths = match provider {
        Provider::Claude => vec![("User settings", home.join(".claude/settings.json"), true)],
        Provider::Copilot => vec![("User settings", home.join(".copilot/settings.json"), true)],
    };
    if let Some(directory) = directory {
        match provider {
            Provider::Claude => {
                paths.push((
                    "Project settings",
                    directory.join(".claude/settings.json"),
                    true,
                ));
                paths.push((
                    "Local project settings",
                    directory.join(".claude/settings.local.json"),
                    true,
                ));
            }
            Provider::Copilot => {
                paths.push((
                    "Project settings",
                    directory.join(".github/copilot/settings.json"),
                    true,
                ));
                paths.push((
                    "Local project settings",
                    directory.join(".github/copilot/settings.local.json"),
                    true,
                ));
            }
        }
    }
    #[cfg(target_os = "macos")]
    if provider == Provider::Claude {
        paths.push((
            "Managed settings",
            PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json"),
            false,
        ));
    }
    let mut unavailable = false;
    let files = paths
        .into_iter()
        .map(|(label, path, allow_edit)| {
            let metadata = std::fs::metadata(&path);
            let exists = metadata.as_ref().is_ok_and(std::fs::Metadata::is_file);
            let readable =
                exists && accessat(CWD, &path, Access::READ_OK, AtFlags::EACCESS).is_ok();
            if metadata
                .as_ref()
                .is_err_and(|error| error.kind() != std::io::ErrorKind::NotFound)
                || (metadata.is_ok() && !readable)
            {
                unavailable = true;
            }
            let editable = readable
                && allow_edit
                && accessat(CWD, &path, Access::WRITE_OK, AtFlags::EACCESS).is_ok();
            let path = if readable {
                path.canonicalize().unwrap_or(path)
            } else {
                path
            };
            ConfigFile {
                label: label.to_owned(),
                path: path.to_string_lossy().into_owned(),
                exists,
                editable,
            }
        })
        .collect();
    let executable = resolve_executable(provider).map(|path| path.to_string_lossy().into_owned());
    let message = if executable.is_none() {
        Some(format!(
            "{} is not available on PATH",
            provider.executable()
        ))
    } else if unavailable {
        Some("Some settings files cannot be opened by this user".to_owned())
    } else {
        None
    };
    Ok(ProviderInfo {
        provider,
        executable,
        files,
        message,
    })
}
