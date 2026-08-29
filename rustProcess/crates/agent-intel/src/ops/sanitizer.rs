use serde_json::Value;

const COPILOT_TOPLEVEL_SECRET_KEYS: &[&str] = &[
    "copilotTokens",
    "tokens",
    "githubToken",
    "githubTokens",
    "lastLoggedInUser",
];

const CLAUDE_TOPLEVEL_SECRET_KEYS: &[&str] = &[
    "apiKeyHelper",
    "awsAuthRefresh",
    "awsCredentialExportCommand",
];

const SECRET_KEY_PATTERNS: &[&str] = &[
    "token",
    "secret",
    "password",
    "credential",
    "apikey",
    "authorization",
];

const SECRET_VALUE_CONTAINERS: &[&str] = &["env", "headers"];

const MAX_SCRUB_DEPTH: usize = 32;

pub fn sanitise_copilot_settings(mut value: Value) -> Value {
    if let Some(map) = value.as_object_mut() {
        for key in COPILOT_TOPLEVEL_SECRET_KEYS {
            map.remove(*key);
        }
    }
    scrub_recursive(&mut value, 0);
    value
}

pub fn sanitise_claude_settings(mut value: Value) -> Value {
    if let Some(map) = value.as_object_mut() {
        for key in CLAUDE_TOPLEVEL_SECRET_KEYS {
            map.remove(*key);
        }
    }
    scrub_recursive(&mut value, 0);
    value
}

fn is_secret_key(key: &str) -> bool {
    let normalised: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    SECRET_KEY_PATTERNS
        .iter()
        .any(|pattern| normalised.contains(pattern))
}

fn is_secret_value_container(key: &str) -> bool {
    SECRET_VALUE_CONTAINERS
        .iter()
        .any(|container| key.eq_ignore_ascii_case(container))
}

fn redact_container_values(value: &mut Value, depth: usize) {
    if depth >= MAX_SCRUB_DEPTH {
        *value = Value::String("<redacted-depth-limit>".to_owned());
        return;
    }
    match value {
        Value::Object(map) => {
            for child in map.values_mut() {
                *child = Value::String("<redacted>".to_owned());
            }
        }
        Value::Array(items) => {
            for child in items {
                *child = Value::String("<redacted>".to_owned());
            }
        }
        _ => *value = Value::String("<redacted>".to_owned()),
    }
}

fn scrub_recursive(value: &mut Value, depth: usize) {
    if depth >= MAX_SCRUB_DEPTH {
        *value = Value::String("<redacted-depth-limit>".to_owned());
        return;
    }
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !is_secret_key(key));
            for (key, child) in map {
                if is_secret_value_container(key) {
                    redact_container_values(child, depth + 1);
                } else {
                    scrub_recursive(child, depth + 1);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub_recursive(item, depth + 1);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_top_level_token_keys() {
        let v = json!({
            "model": "gpt-5.5",
            "copilotTokens": {"oauth_token": "gho_xxx"},
            "lastLoggedInUser": {"login": "octocat"},
            "trustedFolders": ["/Users/john"],
        });
        let cleaned = sanitise_copilot_settings(v);
        let map = cleaned.as_object().expect("object");
        assert!(map.contains_key("model"));
        assert!(map.contains_key("trustedFolders"));
        assert!(!map.contains_key("copilotTokens"));
        assert!(!map.contains_key("lastLoggedInUser"));
    }

    #[test]
    fn strips_nested_secrets() {
        let v = json!({
            "headers": {"Authorization": "Bearer xxx", "X-Api-Key": "k"},
            "auth": {"refreshToken": "r", "accessToken": "a"},
            "env": {"GITHUB_TOKEN": "x", "USER_NAME": "john", "DB_PASSWORD": "p"},
            "model": "gpt-5.5"
        });
        let cleaned = sanitise_copilot_settings(v);
        let s = serde_json::to_string(&cleaned).expect("to_string");
        assert!(!s.contains("Bearer"));
        assert!(!s.contains("refreshToken"));
        assert!(!s.contains("accessToken"));
        assert!(s.contains("GITHUB_TOKEN"));
        assert!(s.contains("DB_PASSWORD"));
        assert!(!s.contains("john"));
        assert!(s.contains("<redacted>"));
        assert!(s.contains("gpt-5.5"));
    }

    #[test]
    fn claude_sanitiser_strips_apikeyhelper_and_aws_refresh() {
        let v = json!({
            "model": "claude-opus-4-7",
            "apiKeyHelper": "/usr/local/bin/fetch-key.sh",
            "awsAuthRefresh": "aws sts assume-role ...",
            "awsCredentialExportCommand": "aws configure export-credentials",
            "includeCoAuthoredBy": true,
        });
        let cleaned = sanitise_claude_settings(v);
        let map = cleaned.as_object().expect("object");
        assert!(map.contains_key("model"));
        assert!(map.contains_key("includeCoAuthoredBy"));
        assert!(!map.contains_key("apiKeyHelper"));
        assert!(!map.contains_key("awsAuthRefresh"));
        assert!(!map.contains_key("awsCredentialExportCommand"));
    }

    #[test]
    fn depth_limit_redacts_entire_subtree() {
        let mut value = json!({"token": "deep-secret"});
        for _ in 0..MAX_SCRUB_DEPTH {
            value = json!({"child": value});
        }
        let cleaned = sanitise_claude_settings(value);
        let encoded = serde_json::to_string(&cleaned).unwrap();
        assert!(!encoded.contains("deep-secret"));
        assert!(encoded.contains("redacted-depth-limit"));
    }

    #[test]
    fn claude_sanitiser_strips_nested_token_keys() {
        let v = json!({
            "env": {"ANTHROPIC_API_KEY": "sk-ant-xxx", "EDITOR": "vim"},
            "permissions": {"allow": ["Bash(git:*)"]},
            "model": "claude-opus-4-7",
            "extras": {"customAuthorizationHeader": "Bearer xxx", "label": "ok"},
        });
        let cleaned = sanitise_claude_settings(v);
        let s = serde_json::to_string(&cleaned).expect("to_string");
        assert!(!s.contains("sk-ant"));
        assert!(s.contains("ANTHROPIC_API_KEY"));
        assert!(!s.contains("customAuthorizationHeader"));
        assert!(!s.contains("Bearer"));
        assert!(s.contains("EDITOR"));
        assert!(!s.contains("vim"));
        assert!(s.contains("<redacted>"));
        assert!(s.contains("permissions"));
        assert!(s.contains("claude-opus-4-7"));
    }
}
