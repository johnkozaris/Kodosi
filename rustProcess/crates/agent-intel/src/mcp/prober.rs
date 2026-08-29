use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum McpHealth {
    #[default]
    Unknown,
    Healthy,
    Unreachable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Misconfigured {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProbeTarget {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

pub async fn probe_passive(target: &McpProbeTarget) -> McpHealth {
    if let Some(command) = &target.command {
        return if resolve_executable(command) {
            McpHealth::Healthy
        } else {
            McpHealth::Misconfigured {
                reason: Some(format!("{command} not on PATH")),
            }
        };
    }
    if let Some(url) = &target.url {
        return probe_http(url).await;
    }
    McpHealth::Misconfigured {
        reason: Some("config has neither `command` nor `url`".to_owned()),
    }
}

fn resolve_executable(command: &str) -> bool {
    crate::runtime::executable::resolve(command).is_some()
}

async fn probe_http(url: &str) -> McpHealth {
    let Some(parsed) = crate::mcp::url::parse_http_url(url) else {
        return McpHealth::Misconfigured {
            reason: Some("URL must be a valid HTTP(S) endpoint".to_owned()),
        };
    };
    let port = parsed
        .port
        .unwrap_or(if parsed.is_https { 443 } else { 80 });
    let addr = format!("{}:{port}", parsed.host);
    let display =
        crate::mcp::url::redact_url_for_display(url).unwrap_or_else(|| "<redacted-url>".to_owned());
    match tokio::time::timeout(PROBE_TIMEOUT, tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => McpHealth::Healthy,
        Ok(Err(err)) => {
            let reason = format!("connect {display}: {err}");
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput
            ) {
                McpHealth::Misconfigured {
                    reason: Some(reason),
                }
            } else {
                McpHealth::Unreachable {
                    reason: Some(reason),
                }
            }
        }
        Err(_) => McpHealth::Unreachable {
            reason: Some(format!(
                "no tcp connect within {}s",
                PROBE_TIMEOUT.as_secs()
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stdio_probe_missing_binary_is_misconfigured() {
        let target = McpProbeTarget {
            name: "missing".into(),
            command: Some("kodosi-no-such-mcp-bin-xyzzy".into()),
            url: None,
        };
        match probe_passive(&target).await {
            McpHealth::Misconfigured { reason } => {
                assert!(reason.unwrap().contains("not on PATH"));
            }
            other => panic!("expected Misconfigured, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn passive_stdio_probe_with_existing_binary_is_healthy() {
        let target = McpProbeTarget {
            name: "true".into(),
            command: Some("true".into()),
            url: None,
        };
        assert_eq!(probe_passive(&target).await, McpHealth::Healthy);
    }

    #[tokio::test]
    async fn http_probe_with_invalid_scheme_is_misconfigured() {
        let target = McpProbeTarget {
            name: "x".into(),
            command: None,
            url: Some("ftp://example.com".into()),
        };
        match probe_passive(&target).await {
            McpHealth::Misconfigured { reason } => {
                let reason = reason.unwrap();
                assert!(reason.contains("HTTP(S)"));
                assert!(!reason.contains("ftp://example.com"));
            }
            other => panic!("expected Misconfigured, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_probe_against_closed_port_is_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let target = McpProbeTarget {
            name: "closed".into(),
            command: None,
            url: Some(format!("http://127.0.0.1:{port}/mcp")),
        };
        match probe_passive(&target).await {
            McpHealth::Unreachable { reason } => {
                let reason = reason.expect("reason");
                assert!(reason.contains(&format!("http://127.0.0.1:{port}")));
                assert!(!reason.contains("/mcp"));
            }
            other => panic!("expected Unreachable on closed port, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_probe_against_open_listener_is_healthy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let target = McpProbeTarget {
            name: "open".into(),
            command: None,
            url: Some(format!("http://127.0.0.1:{port}/mcp")),
        };
        assert_eq!(probe_passive(&target).await, McpHealth::Healthy);
        drop(listener);
    }

    #[test]
    fn shared_url_parser_extracts_host_port_scheme() {
        assert_eq!(
            crate::mcp::url::parse_http_url("HTTPS://user:secret@example.com/path?token=x"),
            Some(crate::mcp::url::ParsedHttpUrl {
                host: "example.com".into(),
                port: None,
                is_https: true,
            })
        );
        assert_eq!(
            crate::mcp::url::parse_http_url("http://localhost:8080/mcp"),
            Some(crate::mcp::url::ParsedHttpUrl {
                host: "localhost".into(),
                port: Some(8080),
                is_https: false,
            })
        );
        assert_eq!(crate::mcp::url::parse_http_url("ftp://example.com"), None);
        assert_eq!(crate::mcp::url::parse_http_url("http://"), None);
    }

    #[tokio::test]
    async fn config_without_command_or_url_is_misconfigured() {
        let target = McpProbeTarget {
            name: "x".into(),
            command: None,
            url: None,
        };
        match probe_passive(&target).await {
            McpHealth::Misconfigured { reason } => {
                assert!(reason.unwrap().contains("neither"));
            }
            other => panic!("expected Misconfigured, got {other:?}"),
        }
    }

    #[test]
    fn default_health_is_unknown() {
        assert_eq!(McpHealth::default(), McpHealth::Unknown);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn passive_stdio_probe_never_executes_configured_program() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("executed");
        let script = temp.path().join("probe");
        std::fs::write(
            &script,
            format!("#!/bin/sh\nprintf x > '{}'\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let target = McpProbeTarget {
            name: "side-effect".into(),
            command: Some(script.to_string_lossy().into_owned()),
            url: None,
        };
        assert_eq!(probe_passive(&target).await, McpHealth::Healthy);
        assert!(!marker.exists());
    }
}
