use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalRisk {
    Destructive,
    Credential,
    Network,
    Safe,

    Unknown,
}

pub(crate) fn classify(tool_name: &str, tool_input: &serde_json::Value) -> ApprovalRisk {
    match tool_name.to_ascii_lowercase().as_str() {
        "read" | "grep" | "glob" | "view" | "search" => ApprovalRisk::Safe,
        "webfetch" | "websearch" | "fetch" => ApprovalRisk::Network,
        "bash" | "shell" | "powershell" | "exec" => classify_bash(
            first_str_field(tool_input, &["command", "cmd", "script"]).unwrap_or_default(),
        ),
        "write" | "edit" | "multiedit" | "notebookedit" | "str_replace_editor" | "create" => {
            classify_file_target(
                first_str_field(tool_input, &["file_path", "path"]).unwrap_or_default(),
            )
        }
        _ if is_network_mcp_tool(tool_name) => ApprovalRisk::Network,
        _ => ApprovalRisk::Unknown,
    }
}

fn first_str_field<'v>(value: &'v serde_json::Value, fields: &[&str]) -> Option<&'v str> {
    fields.iter().find_map(|field| {
        value
            .get(field)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
    })
}

fn is_network_mcp_tool(tool_name: &str) -> bool {
    let Some(rest) = tool_name.strip_prefix("mcp__") else {
        return false;
    };
    let rest = rest.to_ascii_lowercase();
    ["fetch", "http", "web", "browser", "network"]
        .iter()
        .any(|marker| rest.contains(marker))
}

fn classify_bash(command: &str) -> ApprovalRisk {
    let lowered = command.to_ascii_lowercase();
    if is_destructive_bash(&lowered) {
        ApprovalRisk::Destructive
    } else if mentions_credential_path(&lowered) {
        ApprovalRisk::Credential
    } else if reaches_network(&lowered) {
        ApprovalRisk::Network
    } else {
        ApprovalRisk::Unknown
    }
}

fn classify_file_target(file_path: &str) -> ApprovalRisk {
    if mentions_credential_path(&file_path.to_ascii_lowercase()) {
        ApprovalRisk::Credential
    } else {
        ApprovalRisk::Unknown
    }
}

fn is_destructive_bash(lowered: &str) -> bool {
    let rm_rf = lowered
        .split(&[';', '|', '&', '\n'][..])
        .filter_map(|segment| {
            let trimmed = segment.trim();
            command_tokens(trimmed)
                .next()
                .filter(|first| *first == "rm")
                .map(|_| trimmed)
        })
        .any(|rm_invocation| {
            let (mut recursive, mut force) = (false, false);
            for token in command_tokens(rm_invocation).skip(1) {
                if let Some(flags) = token.strip_prefix('-').filter(|f| !f.starts_with('-')) {
                    recursive |= flags.contains('r') || flags.contains('R');
                    force |= flags.contains('f');
                }
                recursive |= token == "--recursive";
                force |= token == "--force";
            }
            recursive && force
        });
    rm_rf
        || has_command_token(lowered, "sudo")
        || (lowered.contains("git push")
            && (has_command_token(lowered, "--force") || has_command_token(lowered, "-f")))
}

fn reaches_network(lowered: &str) -> bool {
    ["curl", "wget", "nc", "ncat", "netcat"]
        .iter()
        .any(|tool| has_command_token(lowered, tool))
}

fn mentions_credential_path(lowered: &str) -> bool {
    const FRAGMENTS: &[&str] = &[
        ".ssh",
        ".env",
        "keychain",
        ".aws/credentials",
        ".netrc",
        ".npmrc",
        "id_rsa",
        "id_ed25519",
        ".pem",
        ".gnupg",
    ];
    FRAGMENTS
        .iter()
        .any(|fragment| contains_path_token(lowered, fragment))
}

fn contains_path_token(haystack: &str, needle: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = haystack[search_from..].find(needle) {
        let end = search_from + pos + needle.len();
        let boundary_after = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if boundary_after {
            return true;
        }
        search_from = end;
    }
    false
}

fn has_command_token(haystack: &str, needle: &str) -> bool {
    command_tokens(haystack).any(|token| token == needle)
}

fn command_tokens(haystack: &str) -> impl Iterator<Item = &str> {
    haystack
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')' | '\'' | '"'))
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classification_table() {
        let cases: &[(&str, serde_json::Value, ApprovalRisk)] = &[
            (
                "Read",
                json!({"file_path": "/tmp/a.rs"}),
                ApprovalRisk::Safe,
            ),
            ("Grep", json!({"pattern": "fn main"}), ApprovalRisk::Safe),
            ("Glob", json!({"pattern": "**/*.rs"}), ApprovalRisk::Safe),
            ("view", json!({"path": "/tmp/a.rs"}), ApprovalRisk::Safe),
            (
                "bash",
                json!({"command": "rm -rf build"}),
                ApprovalRisk::Destructive,
            ),
            (
                "shell",
                json!({"cmd": "curl https://example.com"}),
                ApprovalRisk::Network,
            ),
            (
                "str_replace_editor",
                json!({"path": "/home/u/.ssh/id_ed25519"}),
                ApprovalRisk::Credential,
            ),
            (
                "WebFetch",
                json!({"url": "https://example.com"}),
                ApprovalRisk::Network,
            ),
            ("WebSearch", json!({"query": "rust"}), ApprovalRisk::Network),
            (
                "mcp__fetch__get",
                json!({"url": "https://example.com"}),
                ApprovalRisk::Network,
            ),
            (
                "mcp__browser__navigate",
                json!({"url": "https://example.com"}),
                ApprovalRisk::Network,
            ),
            (
                "mcp__filesystem__read_file",
                json!({"path": "/tmp/a"}),
                ApprovalRisk::Unknown,
            ),
            (
                "Bash",
                json!({"command": "rm -rf /tmp/build"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "rm -fr ./dist"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "rm -r -f ./dist"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "sudo systemctl restart nginx"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "git push --force origin main"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "git push -f"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "cat ~/.ssh/id_ed25519"}),
                ApprovalRisk::Credential,
            ),
            (
                "Bash",
                json!({"command": "cat .env.local"}),
                ApprovalRisk::Credential,
            ),
            (
                "Bash",
                json!({"command": "security find-generic-password keychain"}),
                ApprovalRisk::Credential,
            ),
            (
                "Bash",
                json!({"command": "curl https://example.com"}),
                ApprovalRisk::Network,
            ),
            (
                "Bash",
                json!({"command": "wget https://example.com/x.tar.gz"}),
                ApprovalRisk::Network,
            ),
            (
                "Bash",
                json!({"command": "nc -l 8080"}),
                ApprovalRisk::Network,
            ),
            (
                "Bash",
                json!({"command": "sudo curl https://example.com"}),
                ApprovalRisk::Destructive,
            ),
            (
                "Bash",
                json!({"command": "curl https://example.com -o ~/.ssh/authorized_keys"}),
                ApprovalRisk::Credential,
            ),
            (
                "Bash",
                json!({"command": "rsync -a src/ dst/"}),
                ApprovalRisk::Unknown,
            ),
            (
                "Bash",
                json!({"command": "echo development environment"}),
                ApprovalRisk::Unknown,
            ),
            (
                "Bash",
                json!({"command": "rm -f stale.lock"}),
                ApprovalRisk::Unknown,
            ),
            ("Bash", json!({}), ApprovalRisk::Unknown),
            (
                "Write",
                json!({"file_path": "/home/u/.env"}),
                ApprovalRisk::Credential,
            ),
            (
                "Edit",
                json!({"file_path": "/home/u/.ssh/config"}),
                ApprovalRisk::Credential,
            ),
            (
                "Write",
                json!({"file_path": "/home/u/certs/server.pem"}),
                ApprovalRisk::Credential,
            ),
            (
                "Write",
                json!({"file_path": "/tmp/notes.md"}),
                ApprovalRisk::Unknown,
            ),
            ("Task", json!({"prompt": "explore"}), ApprovalRisk::Unknown),
            ("", serde_json::Value::Null, ApprovalRisk::Unknown),
        ];

        for (tool_name, tool_input, expected) in cases {
            let got = classify(tool_name, tool_input);
            assert_eq!(
                got, *expected,
                "classify({tool_name:?}, {tool_input}) => {got:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn wire_encoding_is_camel_case() {
        for (risk, wire) in [
            (ApprovalRisk::Destructive, "\"destructive\""),
            (ApprovalRisk::Credential, "\"credential\""),
            (ApprovalRisk::Network, "\"network\""),
            (ApprovalRisk::Safe, "\"safe\""),
            (ApprovalRisk::Unknown, "\"unknown\""),
        ] {
            assert_eq!(serde_json::to_string(&risk).expect("serialize"), wire);
        }
    }
}
