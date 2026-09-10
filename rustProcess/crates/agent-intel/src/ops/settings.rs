use std::path::Path;

use tokio::fs;

use crate::AgentKind;
use crate::claude::ClaudeCodeProvider;
use crate::copilot::CopilotCliProvider;
use crate::domain::{SettingsFilePath, SettingsScope};

use super::dto::AgentSettingsBundle;
use super::io::{MAX_AGENT_INTEL_FILE_BYTES, read_bounded_string};
use super::path_safety::{self, PathSafetyError};
use super::sanitizer::{sanitise_claude_settings, sanitise_copilot_settings};

pub async fn read_settings(
    agent: AgentKind,
    cwd: Option<&str>,
) -> Result<AgentSettingsBundle, String> {
    let cwd = cwd.unwrap_or(".");
    let canonical_cwd = if cwd == "." {
        None
    } else {
        Some(
            path_safety::canonicalize_user_dir("cwd", cwd)
                .map_err(|err| format!("invalid cwd: {err}"))?,
        )
    };
    let cwd_for_paths = canonical_cwd.as_ref().map_or_else(
        || ".".to_owned(),
        |path| path.to_string_lossy().into_owned(),
    );

    let paths = agent_settings_paths(agent, &cwd_for_paths);
    let mut bundle = AgentSettingsBundle {
        managed: None,
        user: None,
        project: None,
        local: None,
    };

    for sp in paths {
        if !fs::try_exists(&sp.path).await.unwrap_or(false) {
            continue;
        }

        let must_contain = matches!(sp.scope, SettingsScope::Project | SettingsScope::Local);
        let read_path = if must_contain {
            let Some(root) = canonical_cwd.as_deref() else {
                continue;
            };
            match path_safety::enforce_under_root(root, &sp.path) {
                Ok(canonical) => canonical,
                Err(PathSafetyError::NotFound) => continue,
                Err(err) => {
                    tracing::warn!(
                        path = %sp.path.display(),
                        %err,
                        "read_settings: scope file escaped cwd; skipping"
                    );
                    continue;
                }
            }
        } else {
            sp.path.clone()
        };

        let content = read_bounded_string(&read_path).await?;
        let value = parse_settings_content(&content, &read_path)?;
        let value = match agent {
            AgentKind::Copilot => sanitise_copilot_settings(value),
            AgentKind::Claude => sanitise_claude_settings(value),
        };
        match sp.scope {
            SettingsScope::Managed => bundle.managed = Some(value),
            SettingsScope::User => bundle.user = Some(value),
            SettingsScope::Project => bundle.project = Some(value),
            SettingsScope::Local => bundle.local = Some(value),
        }
    }
    Ok(bundle)
}

pub async fn read_settings_from_root(
    agent: AgentKind,
    root: &std::fs::File,
) -> Result<AgentSettingsBundle, String> {
    let root = root
        .try_clone()
        .map_err(|error| format!("retain project root for settings: {error}"))?;
    tokio::task::spawn_blocking(move || {
        let mut bundle = AgentSettingsBundle {
            managed: None,
            user: None,
            project: None,
            local: None,
        };
        for settings_path in agent_settings_paths(agent, "/") {
            let (content, display_path) = match settings_path.scope {
                SettingsScope::Managed | SettingsScope::User => {
                    let Some(content) = read_bounded_string_sync(&settings_path.path)? else {
                        continue;
                    };
                    (content, settings_path.path)
                }
                SettingsScope::Project | SettingsScope::Local => {
                    let components = relative_settings_components(agent, settings_path.scope);
                    let Some(mut file) = open_relative_regular(&root, components)? else {
                        continue;
                    };
                    let content = read_bounded_file(&mut file)?;
                    (content, Path::new("/").join(components.join("/")))
                }
            };
            let value = parse_settings_content(&content, &display_path)?;
            let value = match agent {
                AgentKind::Copilot => sanitise_copilot_settings(value),
                AgentKind::Claude => sanitise_claude_settings(value),
            };
            match settings_path.scope {
                SettingsScope::Managed => bundle.managed = Some(value),
                SettingsScope::User => bundle.user = Some(value),
                SettingsScope::Project => bundle.project = Some(value),
                SettingsScope::Local => bundle.local = Some(value),
            }
        }
        Ok(bundle)
    })
    .await
    .map_err(|error| format!("bound settings task join: {error}"))?
}

fn relative_settings_components(agent: AgentKind, scope: SettingsScope) -> &'static [&'static str] {
    match (agent, scope) {
        (AgentKind::Claude, SettingsScope::Project) => &[".claude", "settings.json"],
        (AgentKind::Claude, SettingsScope::Local) => &[".claude", "settings.local.json"],
        (AgentKind::Copilot, SettingsScope::Project) => &[".github", "copilot", "settings.json"],
        (AgentKind::Copilot, SettingsScope::Local) => {
            &[".github", "copilot", "settings.local.json"]
        }
        (_, SettingsScope::Managed | SettingsScope::User) => &[],
    }
}

fn open_relative_regular(
    root: &std::fs::File,
    components: &[&str],
) -> Result<Option<std::fs::File>, String> {
    use rustix::fs::{Mode, OFlags};

    let Some((filename, directories)) = components.split_last() else {
        return Err("project settings path is empty".to_owned());
    };
    let mut parent = root
        .try_clone()
        .map_err(|error| format!("retain project settings root: {error}"))?;
    for directory in directories {
        let fd = match rustix::fs::openat(
            &parent,
            *directory,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(error) => return Err(format!("open project settings directory: {error}")),
        };
        parent = std::fs::File::from(fd);
    }
    let fd = match rustix::fs::openat(
        &parent,
        *filename,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(format!("open project settings file: {error}")),
    };
    Ok(Some(std::fs::File::from(fd)))
}

fn read_bounded_string_sync(path: &Path) -> Result<Option<String>, String> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to open file: {error}")),
    };
    read_bounded_file(&mut file).map(Some)
}

fn read_bounded_file(file: &mut std::fs::File) -> Result<String, String> {
    use std::io::Read as _;

    let mut bytes = Vec::new();
    file.take(MAX_AGENT_INTEL_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read file: {error}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_AGENT_INTEL_FILE_BYTES {
        return Err(format!(
            "file too large (> {MAX_AGENT_INTEL_FILE_BYTES} bytes)"
        ));
    }
    String::from_utf8(bytes).map_err(|error| format!("file is not valid UTF-8: {error}"))
}

fn parse_settings_content(content: &str, path: &Path) -> Result<serde_json::Value, String> {
    jsonc_parser::parse_to_serde_value(content, &crate::ops::settings_mutation::parse_options())
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn agent_settings_paths(agent: AgentKind, cwd: &str) -> Vec<SettingsFilePath> {
    match agent {
        AgentKind::Claude => ClaudeCodeProvider::new().settings_paths(cwd),
        AgentKind::Copilot => CopilotCliProvider::new().settings_paths(cwd),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_settings_content, read_settings_from_root};
    use crate::AgentKind;

    #[test]
    fn claude_jsonc_settings_parse_with_comments_and_trailing_commas() {
        let value = parse_settings_content(
            "{ // comment\n \"permissions\": { \"defaultMode\": \"plan\", },\n}",
            std::path::Path::new("settings.json"),
        )
        .expect("valid Claude JSONC");

        assert_eq!(value["permissions"]["defaultMode"], "plan");
    }

    #[tokio::test]
    async fn bound_settings_read_uses_held_project_not_replacement_path() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("project");
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::write(
            project.join(".claude/settings.json"),
            r#"{"permissions":{"defaultMode":"plan"}}"#,
        )
        .unwrap();
        let held = std::fs::File::open(&project).unwrap();
        let retired = parent.path().join("retired");
        std::fs::rename(&project, &retired).unwrap();
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::write(
            project.join(".claude/settings.json"),
            r#"{"permissions":{"defaultMode":"acceptEdits"}}"#,
        )
        .unwrap();

        let settings = read_settings_from_root(AgentKind::Claude, &held)
            .await
            .unwrap();
        assert_eq!(
            settings.project.unwrap()["permissions"]["defaultMode"],
            "plan"
        );
    }
}
