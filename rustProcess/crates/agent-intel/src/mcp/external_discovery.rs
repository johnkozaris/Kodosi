use std::{
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpSourceApp {
    ClaudeDesktop,
    Cursor,
    Windsurf,
}

#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "raw_config: serde_json::Value cannot implement Eq"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredMcpServer {
    pub source_app: McpSourceApp,
    pub config_path: PathBuf,
    pub server_name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,

    pub raw_config: serde_json::Value,
}

#[must_use]
pub fn discover_external_mcp_servers(home: &Path) -> Vec<DiscoveredMcpServer> {
    let mut out = Vec::new();
    for (app, path) in default_search_paths(home) {
        scan_file(app, &path, &mut out);
    }
    out
}

#[must_use]
pub fn default_search_paths(home: &Path) -> Vec<(McpSourceApp, PathBuf)> {
    let mut out = vec![
        (McpSourceApp::Cursor, home.join(".cursor").join("mcp.json")),
        (
            McpSourceApp::Windsurf,
            home.join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
        ),
    ];
    out.insert(0, (McpSourceApp::ClaudeDesktop, claude_desktop_path(home)));
    out
}

#[cfg(target_os = "macos")]
fn claude_desktop_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("Claude")
        .join("claude_desktop_config.json")
}

#[cfg(not(target_os = "macos"))]
fn claude_desktop_path(home: &Path) -> PathBuf {
    let xdg =
        std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from);
    xdg.join("Claude").join("claude_desktop_config.json")
}

const MAX_EXTERNAL_MCP_CONFIG_BYTES: u64 = 1024 * 1024;

fn scan_file(app: McpSourceApp, path: &Path, out: &mut Vec<DiscoveredMcpServer>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let mut raw = String::new();
    if file
        .take(MAX_EXTERNAL_MCP_CONFIG_BYTES.saturating_add(1))
        .read_to_string(&mut raw)
        .is_err()
        || u64::try_from(raw.len()).unwrap_or(u64::MAX) > MAX_EXTERNAL_MCP_CONFIG_BYTES
    {
        return;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    let servers = match value
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
    {
        Some(map) => map.clone(),
        None => match value.as_object() {
            Some(obj) => obj.clone(),
            None => return,
        },
    };
    for (name, cfg) in servers {
        let looks_like_server =
            cfg.get("command").is_some() || cfg.get("url").is_some() || cfg.get("type").is_some();
        if !looks_like_server {
            continue;
        }
        let transport = if cfg.get("url").is_some() {
            Some("http".to_owned())
        } else if cfg.get("command").is_some() {
            Some("stdio".to_owned())
        } else {
            cfg.get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        out.push(DiscoveredMcpServer {
            source_app: app,
            config_path: path.to_owned(),
            server_name: name,
            transport,
            raw_config: redact_mcp_config(cfg),
        });
    }
}

fn redact_mcp_config(mut value: serde_json::Value) -> serde_json::Value {
    redact_value(&mut value, 0, None);
    value
}

fn redact_value(value: &mut serde_json::Value, depth: usize, parent_key: Option<&str>) {
    const MAX_DEPTH: usize = 32;
    if depth >= MAX_DEPTH {
        *value = serde_json::Value::String("<redacted>".to_owned());
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let normalized = key
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                let secret_key = [
                    "token",
                    "secret",
                    "password",
                    "apikey",
                    "authorization",
                    "credential",
                ]
                .iter()
                .any(|pattern| normalized.contains(pattern));
                let secret_container = matches!(
                    normalized.as_str(),
                    "env" | "headers" | "header" | "environment"
                );
                if secret_key {
                    let safe_placeholder = child
                        .as_str()
                        .is_some_and(|text| text.starts_with("${") && text.ends_with('}'));
                    if !safe_placeholder {
                        *child = serde_json::Value::String("<redacted>".to_owned());
                    }
                } else {
                    redact_value(
                        child,
                        depth + 1,
                        parent_key.or_else(|| secret_container.then_some(normalized.as_str())),
                    );
                }
            }
        }
        serde_json::Value::Array(items) => {
            let mut redact_next = false;
            for item in items {
                if redact_next {
                    *item = serde_json::Value::String("<redacted>".to_owned());
                    redact_next = false;
                    continue;
                }
                if let Some(text) = item.as_str() {
                    if is_header_flag(text) {
                        redact_next = true;
                        *item = serde_json::Value::String("<redacted>".to_owned());
                        continue;
                    }
                    if contains_inline_credential(text) {
                        redact_next =
                            text.starts_with('-') && is_secret_flag(text) && !text.contains('=');
                        *item = serde_json::Value::String("<redacted>".to_owned());
                        continue;
                    }
                }
                redact_value(item, depth + 1, parent_key);
            }
        }
        serde_json::Value::String(text) => {
            if let Some(sanitized) = crate::mcp::url::redact_url_scalar(text) {
                *text = sanitized;
                return;
            }
            if parent_key.is_some()
                && !(text.starts_with("${") && text.ends_with('}') && text.len() > 3)
            {
                "<redacted>".clone_into(text);
            }
        }

        _ => {
            if parent_key.is_some() {
                *value = serde_json::Value::String("<redacted>".to_owned());
            }
        }
    }
}

fn is_header_flag(value: &str) -> bool {
    matches!(value, "--header" | "-H")
}

fn is_secret_flag(value: &str) -> bool {
    let flag = value
        .trim_start_matches('-')
        .split_once('=')
        .map_or_else(|| value.trim_start_matches('-'), |(name, _)| name)
        .replace('_', "-")
        .to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "api-key",
        "apikey",
        "authorization",
        "credential",
        "bearer",
    ]
    .iter()
    .any(|marker| flag.contains(marker))
}

fn contains_inline_credential(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    is_secret_flag(value)
        || normalized.starts_with("--header=")
        || normalized.starts_with("-h=")
        || is_attached_header_argument(value)
        || normalized.starts_with("authorization:")
        || normalized.starts_with("authorization=")
        || normalized.starts_with("bearer ")
        || normalized.contains(" authorization: bearer ")
        || normalized.contains(" authorization=bearer ")
        || normalized.contains("x-api-key:")
        || normalized.contains("x-api-key=")
        || normalized.contains("api-key:")
        || normalized.contains("api-key=")
        || normalized.contains("token:")
        || normalized.contains("token=")
        || normalized.contains("secret:")
        || normalized.contains("secret=")
}

fn is_attached_header_argument(value: &str) -> bool {
    value.trim().strip_prefix("-H").is_some_and(|header| {
        header
            .split_once(':')
            .is_some_and(|(name, value)| !name.is_empty() && !value.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn canonical_shape_is_parsed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".cursor").join("mcp.json");
        write(
            &path,
            r#"{
                "mcpServers": {
                    "fs": { "command": "mcp-server-filesystem", "args": ["/tmp"] },
                    "github": { "url": "https://example.com/mcp" }
                }
            }"#,
        );
        let mut out = Vec::new();
        scan_file(McpSourceApp::Cursor, &path, &mut out);
        assert_eq!(out.len(), 2);
        assert!(
            out.iter()
                .any(|s| s.server_name == "fs" && s.transport.as_deref() == Some("stdio"))
        );
        assert!(
            out.iter()
                .any(|s| s.server_name == "github" && s.transport.as_deref() == Some("http"))
        );
    }

    #[test]
    fn flat_shape_with_command_is_parsed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("flat.json");
        write(
            &path,
            r#"{
                "fs": { "command": "mcp-server-filesystem" },
                "version": "1.2.3"
            }"#,
        );
        let mut out = Vec::new();
        scan_file(McpSourceApp::Cursor, &path, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].server_name, "fs");
    }

    #[test]
    fn missing_or_unreadable_file_is_silent() {
        let mut out = Vec::new();
        scan_file(
            McpSourceApp::Cursor,
            Path::new("/definitely/not/a/real/path/mcp.json"),
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn oversized_config_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oversized.json");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_EXTERNAL_MCP_CONFIG_BYTES + 1).unwrap();
        let mut out = Vec::new();

        scan_file(McpSourceApp::Cursor, &path, &mut out);

        assert!(out.is_empty());
    }

    #[test]
    fn raw_config_preserves_env_interpolation_markers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cfg.json");
        write(
            &path,
            r#"{
                "mcpServers": {
                    "x": {
                        "command": "/bin/true",
                        "env": { "TOKEN": "${HOST_TOKEN}" }
                    }
                }
            }"#,
        );
        let mut out = Vec::new();
        scan_file(McpSourceApp::Cursor, &path, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].raw_config["env"]["TOKEN"], "${HOST_TOKEN}",
            "env interpolation markers must survive into raw_config so embedders can translate at adoption time"
        );
    }

    #[test]
    fn raw_config_redacts_literal_secrets_and_sensitive_args() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cfg.json");
        write(
            &path,
            r#"{"mcpServers":{"x":{"command":"/bin/true","env":{"TOKEN":"literal","SAFE":"${SAFE}"},"args":["--token","literal"]}}}"#,
        );
        let mut out = Vec::new();
        scan_file(McpSourceApp::Cursor, &path, &mut out);
        assert_eq!(out[0].raw_config["env"]["TOKEN"], "<redacted>");
        assert_eq!(out[0].raw_config["env"]["SAFE"], "${SAFE}");
        assert_eq!(out[0].raw_config["args"][0], "<redacted>");
        assert_eq!(out[0].raw_config["args"][1], "<redacted>");
    }

    #[test]
    fn raw_config_redacts_header_and_bearer_credentials_in_argument_arrays() {
        let raw = serde_json::json!({
            "command": "server",
            "args": [
                "--header", "Authorization: Bearer first",
                "-H", "X-Api-Key: second",
                "--header=Authorization: Bearer third",
                "-HAuthorization: Bearer fourth",
                "Bearer fifth",
                "--token=token-six",
                "--client-secret", "secret-seven"
            ]
        });

        let redacted = redact_mcp_config(raw);
        assert!(
            redacted["args"]
                .as_array()
                .expect("args")
                .iter()
                .all(|value| value == "<redacted>"),
            "no header, bearer, token, or secret credential may survive in raw_config"
        );
    }

    #[test]
    fn raw_config_never_exposes_header_credentials_in_object_or_array_forms() {
        let raw = serde_json::json!({
            "command": "server",
            "headers": {
                "Authorization": "Bearer object-secret",
                "X-Api-Key": "object-api-key"
            },
            "header": ["Authorization: Bearer array-secret"]
        });

        let redacted = redact_mcp_config(raw);
        let serialized = serde_json::to_string(&redacted).expect("redacted config serializes");
        for secret in ["object-secret", "object-api-key", "array-secret"] {
            assert!(
                !serialized.contains(secret),
                "raw_config leaked header credential {secret}"
            );
        }
    }

    #[test]
    fn raw_config_redacts_prefixed_token_and_bearer_argument_forms() {
        let redacted = redact_mcp_config(serde_json::json!({
            "command": "server",
            "args": [
                "--github_token", "token-value",
                "--bearer", "bearer-value",
                "Authorization Bearer inline-value",
                "-H=Authorization: Basic header-value"
            ]
        }));

        assert!(
            redacted["args"]
                .as_array()
                .expect("args")
                .iter()
                .all(|value| value == "<redacted>")
        );
    }

    #[test]
    fn raw_config_redacts_every_attached_short_header_form() {
        let redacted = redact_mcp_config(serde_json::json!({
            "command": "server",
            "args": [
                "-HX-Custom-Header: arbitrary-value",
                "-HAccept: application/json",
                "-HX-Trace-Context: tenant=example"
            ]
        }));

        assert!(
            redacted["args"]
                .as_array()
                .expect("args")
                .iter()
                .all(|value| value == "<redacted>")
        );
    }

    #[test]
    fn raw_config_sanitizes_url_secrets_in_url_and_arbitrary_scalar_values() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cfg.json");
        write(
            &path,
            r#"{"mcpServers":{"x":{
                "command":"/bin/true",
                "url":"https://user:password@example.test/mcp?token=secret#fragment",
                "rawConfig":"https://alice:secret@other.test/path?api_key=secret#tail"
            }}}"#,
        );
        let mut out = Vec::new();
        scan_file(McpSourceApp::Cursor, &path, &mut out);
        assert_eq!(out[0].raw_config["url"], "https://example.test");
        assert_eq!(out[0].raw_config["rawConfig"], "https://other.test");
    }

    #[test]
    fn default_paths_includes_cursor_and_windsurf() {
        let temp = tempfile::tempdir().unwrap();
        let paths = default_search_paths(temp.path());
        let apps: Vec<McpSourceApp> = paths.iter().map(|(a, _)| *a).collect();
        assert!(apps.contains(&McpSourceApp::Cursor));
        assert!(apps.contains(&McpSourceApp::Windsurf));
    }

    #[test]
    fn discover_returns_empty_on_fresh_home() {
        let temp = tempfile::tempdir().unwrap();
        let found = discover_external_mcp_servers(temp.path());
        assert!(found.is_empty());
    }
}
