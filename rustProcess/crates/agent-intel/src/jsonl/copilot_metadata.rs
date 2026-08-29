pub fn extract_start_model(data: &serde_json::Value) -> Option<String> {
    data.get("selectedModel")
        .or_else(|| data.get("model"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            data.get("context")
                .and_then(|c| c.get("model"))
                .and_then(serde_json::Value::as_str)
        })
        .map(ToOwned::to_owned)
}

pub fn extract_new_model(data: &serde_json::Value) -> Option<String> {
    data.get("newModel")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn extract_copilot_version(data: &serde_json::Value) -> Option<String> {
    data.get("copilotVersion")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn extract_cwd(data: &serde_json::Value) -> Option<String> {
    data.get("context")
        .and_then(|c| c.get("cwd"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| data.get("cwd").and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_selected_start_model_from_current_fixture() {
        let data = serde_json::json!({
            "selectedModel": "gpt-5.6-sol",
            "context": { "model": "legacy" }
        });
        assert_eq!(extract_start_model(&data).as_deref(), Some("gpt-5.6-sol"));
    }

    #[test]
    fn extracts_start_model_from_nested_context() {
        let data = serde_json::json!({ "context": { "model": "gpt-5.5" } });
        assert_eq!(extract_start_model(&data).as_deref(), Some("gpt-5.5"));
    }

    #[test]
    fn extracts_new_model_from_model_change() {
        let data = serde_json::json!({ "newModel": "claude-opus-4.7" });
        assert_eq!(extract_new_model(&data).as_deref(), Some("claude-opus-4.7"));
    }

    #[test]
    fn extracts_version_and_cwd() {
        let data = serde_json::json!({
            "copilotVersion": "1.0.36-1",
            "context": { "cwd": "/tmp/proj" }
        });
        assert_eq!(extract_copilot_version(&data).as_deref(), Some("1.0.36-1"));
        assert_eq!(extract_cwd(&data).as_deref(), Some("/tmp/proj"));
    }

    #[test]
    fn missing_fields_return_none() {
        let data = serde_json::json!({});
        assert!(extract_start_model(&data).is_none());
        assert!(extract_new_model(&data).is_none());
        assert!(extract_copilot_version(&data).is_none());
        assert!(extract_cwd(&data).is_none());
    }
}
