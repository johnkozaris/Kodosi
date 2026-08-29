use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::mcp::{McpProbeTarget, enumerate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedMcpTarget {
    pub vendor: String,

    pub scope: String,
    pub name: String,
    #[serde(flatten)]
    pub target: McpProbeTarget,
}

#[must_use]
pub fn enumerate_user_scope_targets(
    claude_home: &Path,
    copilot_home: &Path,
) -> Vec<ScopedMcpTarget> {
    let mut out = Vec::new();

    out.extend(into_targets(
        "claude",
        enumerate::enumerate_claude(claude_home, None),
    ));
    out.extend(into_targets(
        "copilot",
        enumerate::enumerate_copilot(copilot_home, None),
    ));
    out
}

fn into_targets(vendor: &str, entries: Vec<enumerate::McpServerEntry>) -> Vec<ScopedMcpTarget> {
    entries
        .into_iter()
        .filter_map(|entry| {
            let target = entry.probe_target()?;
            Some(ScopedMcpTarget {
                vendor: vendor.to_owned(),
                scope: entry.scope.label().to_owned(),
                name: entry.name,
                target,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, body).expect("write");
    }

    #[test]
    fn empty_when_neither_config_exists() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        let targets = enumerate_user_scope_targets(&claude, &copilot);
        assert!(targets.is_empty());
    }

    #[test]
    fn picks_up_claude_user_scope() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        write(
            &claude.join("mcp.json"),
            r#"{"mcpServers":{"echo":{"command":"/bin/echo","args":["hi"]}}}"#,
        );
        let targets = enumerate_user_scope_targets(&claude, &copilot);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].scope, "user");
        assert_eq!(targets[0].name, "echo");
        assert_eq!(targets[0].target.command.as_deref(), Some("/bin/echo"));
    }

    #[test]
    fn merges_claude_and_copilot_user_scope() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        write(
            &claude.join("mcp.json"),
            r#"{"mcpServers":{"a":{"command":"/bin/true"}}}"#,
        );
        write(
            &copilot.join("mcp-config.json"),
            r#"{"mcpServers":{"b":{"url":"https://example.test"}}}"#,
        );
        let targets = enumerate_user_scope_targets(&claude, &copilot);
        let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(targets.iter().all(|t| t.scope == "user"));
    }

    #[test]
    fn skips_entries_with_neither_command_nor_url() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        write(
            &claude.join("mcp.json"),
            r#"{"mcpServers":{"broken":{"description":"no transport"}}}"#,
        );
        let targets = enumerate_user_scope_targets(&claude, &copilot);
        assert!(
            targets.is_empty(),
            "entries without command/url are noise, not probeable"
        );
    }

    #[test]
    fn malformed_json_is_silently_skipped() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        write(&claude.join("mcp.json"), "{ not json");
        let targets = enumerate_user_scope_targets(&claude, &copilot);
        assert!(targets.is_empty());
    }
}
