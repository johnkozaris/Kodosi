pub fn extract_custom_title(value: &serde_json::Value) -> Option<String> {
    let record_type = value.get("type")?.as_str()?;
    if record_type != "custom-title" {
        return None;
    }
    value
        .get("customTitle")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_custom_title() {
        let value = serde_json::json!({ "type": "custom-title", "customTitle": "My Session" });
        assert_eq!(extract_custom_title(&value), Some("My Session".to_owned()));
    }
}
