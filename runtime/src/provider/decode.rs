use std::collections::HashSet;

use serde_json::Value;

use super::{ConversationEntry, Provider};

const MAX_PREVIEW_BYTES: usize = 1024 * 1024 - 1024;

#[derive(Default)]
struct Preview {
    entries: Vec<ConversationEntry>,
    encoded_bytes: usize,
}

pub(super) fn conversation(
    provider: Provider,
    raw: &str,
) -> Result<Vec<ConversationEntry>, String> {
    let mut preview = Preview::default();
    let mut tool_ids = HashSet::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|error| format!("Malformed transcript record {}: {error}", index + 1))?;
        if !value.is_object() {
            return Err(format!("Transcript record {} is not an object", index + 1));
        }
        let timestamp = value.get("timestamp").and_then(Value::as_str);
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match provider {
            Provider::Claude => claude_record(&value, kind, timestamp, &mut preview)?,
            Provider::Copilot => {
                copilot_record(&value, kind, timestamp, &mut preview, &mut tool_ids)?;
            }
        }
    }
    Ok(preview.entries)
}

fn claude_record(
    value: &Value,
    kind: &str,
    timestamp: Option<&str>,
    preview: &mut Preview,
) -> Result<(), String> {
    let content = value
        .get("message")
        .and_then(|message| message.get("content"));
    match kind {
        "user" => {
            let text = text_content(content);
            if !text.starts_with("<local-command") {
                push(preview, "user", text, None, timestamp)?;
            }
        }
        "assistant" => {
            if let Some(blocks) = content.and_then(Value::as_array) {
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => push(
                            preview,
                            "assistant",
                            text_content(block.get("text")),
                            None,
                            timestamp,
                        )?,
                        Some("tool_use") => tool(
                            preview,
                            block.get("name").and_then(Value::as_str),
                            block.get("input"),
                            timestamp,
                        )?,
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn copilot_record(
    value: &Value,
    kind: &str,
    timestamp: Option<&str>,
    preview: &mut Preview,
    tool_ids: &mut HashSet<String>,
) -> Result<(), String> {
    let data = value.get("data").unwrap_or(&Value::Null);
    match kind {
        "user.message" => push(
            preview,
            "user",
            text_content(data.get("content")),
            None,
            timestamp,
        )?,
        "assistant.message" => {
            push(
                preview,
                "assistant",
                text_content(data.get("content")),
                None,
                timestamp,
            )?;
            if let Some(requests) = data.get("toolRequests").and_then(Value::as_array) {
                for request in requests {
                    if let Some(id) = request.get("toolCallId").and_then(Value::as_str) {
                        tool_ids.insert(id.to_owned());
                    }
                    tool(
                        preview,
                        request.get("name").and_then(Value::as_str),
                        request.get("arguments"),
                        timestamp,
                    )?;
                }
            }
        }
        "tool.execution_start" => {
            let id = data.get("toolCallId").and_then(Value::as_str);
            if id.is_none_or(|id| tool_ids.insert(id.to_owned())) {
                tool(
                    preview,
                    data.get("toolName").and_then(Value::as_str),
                    data.get("arguments"),
                    timestamp,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn text_content(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn push(
    preview: &mut Preview,
    role: &str,
    content: String,
    tool_name: Option<String>,
    timestamp: Option<&str>,
) -> Result<(), String> {
    if content.is_empty() && tool_name.is_none() {
        return Ok(());
    }
    let entry = ConversationEntry {
        role: role.to_owned(),
        content,
        tool_name,
        timestamp: timestamp.map(str::to_owned),
    };
    let bytes = serde_json::to_vec(&entry)
        .map_err(|error| error.to_string())?
        .len()
        + 1;
    if preview.encoded_bytes.saturating_add(bytes) > MAX_PREVIEW_BYTES {
        return Err(
            "Decoded conversation exceeds the preview byte limit; request fewer records".to_owned(),
        );
    }
    preview.encoded_bytes += bytes;
    preview.entries.push(entry);
    Ok(())
}

fn tool(
    preview: &mut Preview,
    name: Option<&str>,
    arguments: Option<&Value>,
    timestamp: Option<&str>,
) -> Result<(), String> {
    let summary = arguments
        .and_then(Value::as_object)
        .and_then(|arguments| {
            [
                "command",
                "filePath",
                "file_path",
                "path",
                "description",
                "query",
                "intent",
            ]
            .iter()
            .find_map(|key| {
                arguments
                    .get(*key)
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
            })
        })
        .unwrap_or_default();
    let mut summary = redact(summary);
    if summary.len() > 1024 {
        summary.truncate(summary.floor_char_boundary(1024));
        summary.push('…');
    }
    push(
        preview,
        "tool_use",
        summary,
        Some(name.unwrap_or("tool").to_owned()),
        timestamp,
    )
}

fn redact(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut redact_next = false;
    for segment in value.split_inclusive(char::is_whitespace) {
        let token = segment.trim_end_matches(char::is_whitespace);
        let whitespace = &segment[token.len()..];
        if redact_next {
            output.push_str("<redacted>");
            redact_next = false;
        } else if ["gho_", "ghp_", "ghs_", "ghu_", "ghr_", "github_pat_", "sk-"]
            .iter()
            .any(|prefix| token.starts_with(prefix) && token.len() > prefix.len() + 4)
        {
            output.push_str("<redacted>");
        } else if let Some((name, _)) = token.split_once('=')
            && ["TOKEN", "KEY", "SECRET", "PASSWORD", "PASSWD"]
                .iter()
                .any(|key| name.to_ascii_uppercase().contains(key))
        {
            output.push_str(name);
            output.push_str("=<redacted>");
        } else {
            output.push_str(token);
            redact_next =
                token.eq_ignore_ascii_case("Bearer") || token.eq_ignore_ascii_case("Basic");
        }
        output.push_str(whitespace);
    }
    output
}
