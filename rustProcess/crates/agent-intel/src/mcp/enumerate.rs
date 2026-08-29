use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::mcp::McpProbeTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpScope {
    User,
    Project,
}

impl McpScope {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerEntry {
    pub scope: McpScope,
    pub name: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub source_path: PathBuf,
}

impl McpServerEntry {
    #[must_use]
    pub fn transport(&self) -> Option<&'static str> {
        if self.command.is_some() {
            Some("stdio")
        } else if self.url.is_some() {
            Some("http")
        } else {
            None
        }
    }

    #[must_use]
    pub fn probe_target(&self) -> Option<McpProbeTarget> {
        if self.command.is_none() && self.url.is_none() {
            return None;
        }
        Some(McpProbeTarget {
            name: self.name.clone(),
            command: self.command.clone(),
            url: self.url.clone(),
        })
    }
}

#[must_use]
pub fn enumerate_claude(claude_home: &Path, cwd: Option<&Path>) -> Vec<McpServerEntry> {
    let mut out = Vec::new();
    read_mcp_file(&claude_home.join("mcp.json"), McpScope::User, &mut out);
    if let Some(cwd) = cwd {
        read_mcp_file(&cwd.join(".mcp.json"), McpScope::Project, &mut out);
    }
    out
}

#[must_use]
pub fn enumerate_copilot(copilot_home: &Path, cwd: Option<&Path>) -> Vec<McpServerEntry> {
    let mut out = Vec::new();
    read_mcp_file(
        &copilot_home.join("mcp-config.json"),
        McpScope::User,
        &mut out,
    );
    if let Some(cwd) = cwd {
        read_mcp_file(&cwd.join(".mcp.json"), McpScope::Project, &mut out);
        read_mcp_file(
            &cwd.join(".github").join("mcp.json"),
            McpScope::Project,
            &mut out,
        );
    }
    out
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct McpFileView {
    #[serde(default)]
    mcp_servers: std::collections::BTreeMap<String, McpServerRecord>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct McpServerRecord {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    command: Option<String>,
}

fn read_mcp_file(path: &Path, scope: McpScope, out: &mut Vec<McpServerEntry>) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(view) = serde_json::from_slice::<McpFileView>(&bytes) else {
        return;
    };
    for (name, record) in view.mcp_servers {
        out.push(McpServerEntry {
            scope,
            name,
            command: record.command,
            url: record.url,
            source_path: path.to_path_buf(),
        });
    }
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
    fn scope_labels_are_unified() {
        assert_eq!(McpScope::User.label(), "user");
        assert_eq!(McpScope::Project.label(), "project");
    }

    #[test]
    fn empty_when_no_files_exist() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        let cwd = temp.path().join("ws");
        assert!(enumerate_claude(&claude, Some(&cwd)).is_empty());
        assert!(enumerate_copilot(&copilot, Some(&cwd)).is_empty());
    }

    #[test]
    fn claude_picks_up_user_and_project_scope() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        let cwd = temp.path().join("ws");
        write(
            &claude.join("mcp.json"),
            r#"{"mcpServers":{"user-srv":{"command":"/bin/echo"}}}"#,
        );
        write(
            &cwd.join(".mcp.json"),
            r#"{"mcpServers":{"proj-srv":{"url":"https://example.test/mcp"}}}"#,
        );
        let entries = enumerate_claude(&claude, Some(&cwd));
        assert_eq!(entries.len(), 2);
        let user = entries.iter().find(|e| e.name == "user-srv").unwrap();
        assert_eq!(user.scope, McpScope::User);
        assert_eq!(user.command.as_deref(), Some("/bin/echo"));
        let proj = entries.iter().find(|e| e.name == "proj-srv").unwrap();
        assert_eq!(proj.scope, McpScope::Project);
        assert_eq!(proj.transport(), Some("http"));
    }

    #[test]
    fn copilot_reads_both_dotmcp_and_github_mcp_for_project() {
        let temp = TempDir::new().unwrap();
        let copilot = temp.path().join(".copilot");
        let cwd = temp.path().join("ws");
        write(
            &cwd.join(".mcp.json"),
            r#"{"mcpServers":{"a":{"command":"a"}}}"#,
        );
        write(
            &cwd.join(".github").join("mcp.json"),
            r#"{"mcpServers":{"b":{"command":"b"}}}"#,
        );
        let entries = enumerate_copilot(&copilot, Some(&cwd));
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(entries.iter().all(|e| e.scope == McpScope::Project));
    }

    #[test]
    fn malformed_json_is_silently_skipped() {
        let temp = TempDir::new().unwrap();
        let claude = temp.path().join(".claude");
        write(&claude.join("mcp.json"), "{not json");
        let entries = enumerate_claude(&claude, None);
        assert!(entries.is_empty());
    }

    #[test]
    fn entry_without_command_or_url_yields_no_probe_target() {
        let entry = McpServerEntry {
            scope: McpScope::User,
            name: "x".into(),
            command: None,
            url: None,
            source_path: PathBuf::new(),
        };
        assert!(entry.probe_target().is_none());
        assert!(entry.transport().is_none());
    }
}
