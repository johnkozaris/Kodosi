use tokio::fs;

use crate::AgentKind;
use crate::claude::ClaudeCodeProvider;
use crate::copilot::CopilotCliProvider;
use crate::domain::{SettingsFilePath, SettingsScope};

use super::dto::AgentSettingsBundle;
use super::io::read_bounded_string;
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

fn parse_settings_content(
    content: &str,
    path: &std::path::Path,
) -> Result<serde_json::Value, String> {
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
    use super::parse_settings_content;

    #[test]
    fn claude_jsonc_settings_parse_with_comments_and_trailing_commas() {
        let value = parse_settings_content(
            "{ // comment\n \"permissions\": { \"defaultMode\": \"plan\", },\n}",
            std::path::Path::new("settings.json"),
        )
        .expect("valid Claude JSONC");

        assert_eq!(value["permissions"]["defaultMode"], "plan");
    }
}
