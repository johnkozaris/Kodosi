use std::path::{Path, PathBuf};

use super::{ConfigFile, Provider, ProviderInfo};

pub(super) fn resolve_executable(provider: Provider) -> Option<PathBuf> {
    kodosi_os::process::find_executable(provider.executable())
}

pub(super) fn inspect(
    state_root: &Path,
    provider: Provider,
    directory: Option<&Path>,
) -> ProviderInfo {
    let mut paths = vec![("User settings", state_root.join("settings.json"), true)];
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
            let readable = exists && kodosi_os::fs::can_read(&path);
            if metadata
                .as_ref()
                .is_err_and(|error| error.kind() != std::io::ErrorKind::NotFound)
                || (metadata.is_ok() && !readable)
            {
                unavailable = true;
            }
            let editable = readable && allow_edit && kodosi_os::fs::can_write(&path);
            let path = if readable {
                kodosi_os::fs::canonicalize(&path).unwrap_or(path)
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
    ProviderInfo {
        provider,
        executable,
        files,
        message,
    }
}
