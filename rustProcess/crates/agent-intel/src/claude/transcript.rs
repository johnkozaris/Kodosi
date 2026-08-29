use serde_json::Value;

use crate::transcript::{ConversationEntry, DecodeError, TranscriptDecoder};

const MAX_TOOL_SUMMARY_BYTES: usize = 1024;

const LOCAL_COMMAND_PREFIX: &str = "<local-command";

#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeTranscriptDecoder;

impl ClaudeTranscriptDecoder {
    pub const INSTANCE: &'static Self = &Self;
}

impl TranscriptDecoder for ClaudeTranscriptDecoder {
    fn decode_conversation(&self, raw: &str) -> Result<Vec<ConversationEntry>, DecodeError> {
        let mut entries = Vec::new();

        for line in raw.lines() {
            let Some(value) = parse_jsonl_line(line) else {
                continue;
            };

            let record_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let timestamp = value
                .get("timestamp")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);

            match record_type {
                "user" => decode_user(&value, timestamp, &mut entries),
                "assistant" => decode_assistant(&value, timestamp.as_deref(), &mut entries),
                _ => {}
            }
        }

        Ok(entries)
    }
}

fn decode_user(value: &Value, timestamp: Option<String>, entries: &mut Vec<ConversationEntry>) {
    let msg = value.get("message").and_then(|m| m.get("content"));
    let text = if let Some(s) = msg.and_then(Value::as_str) {
        s.to_owned()
    } else if let Some(blocks) = msg.and_then(Value::as_array) {
        blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    b.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        return;
    };
    if text.is_empty() || text.starts_with(LOCAL_COMMAND_PREFIX) {
        return;
    }
    entries.push(ConversationEntry {
        role: "user".to_owned(),
        content: text,
        tool_name: None,
        timestamp,
    });
}

fn decode_assistant(value: &Value, timestamp: Option<&str>, entries: &mut Vec<ConversationEntry>) {
    let Some(blocks) = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };

    for block in blocks {
        let bt = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match bt {
            "text" => {
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !text.is_empty() {
                    entries.push(ConversationEntry {
                        role: "assistant".to_owned(),
                        content: text.to_owned(),
                        tool_name: None,
                        timestamp: timestamp.map(str::to_owned),
                    });
                }
            }
            "tool_use" => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                let summary = block
                    .get("input")
                    .and_then(|i| {
                        i.get("command")
                            .or_else(|| i.get("file_path"))
                            .or_else(|| i.get("description"))
                            .and_then(Value::as_str)
                    })
                    .map(|value| truncate_summary(&crate::jsonl::scrub_secret_patterns(value)))
                    .unwrap_or_default();
                entries.push(ConversationEntry {
                    role: "tool_use".to_owned(),
                    content: summary,
                    tool_name: Some(name.to_owned()),
                    timestamp: timestamp.map(str::to_owned),
                });
            }
            _ => {}
        }
    }
}

fn parse_jsonl_line(line: &str) -> Option<Value> {
    serde_json::from_str(line).ok()
}

fn truncate_summary(s: &str) -> String {
    if s.len() <= MAX_TOOL_SUMMARY_BYTES {
        return s.to_owned();
    }

    let end = s.floor_char_boundary(MAX_TOOL_SUMMARY_BYTES);
    let mut out = String::with_capacity(end + 1);
    out.push_str(&s[..end]);
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(raw: &str) -> Vec<ConversationEntry> {
        ClaudeTranscriptDecoder
            .decode_conversation(raw)
            .expect("claude decoder never returns Err")
    }

    #[test]
    fn bash_summary_scrubs_secret_shaped_arguments() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"curl -H 'Authorization: Bearer secret-value' https://example.test API_TOKEN=abc123"}}]}}"#;
        let output = decode(raw);
        assert_eq!(output.len(), 1);
        assert!(!output[0].content.contains("secret-value"));
        assert!(!output[0].content.contains("abc123"));
        assert!(output[0].content.contains("<redacted>"));
    }

    #[test]
    fn user_string_content() {
        let raw =
            r#"{"type":"user","message":{"content":"hello"},"timestamp":"2024-01-01T00:00:00Z"}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content, "hello");
        assert_eq!(out[0].timestamp.as_deref(), Some("2024-01-01T00:00:00Z"));
    }

    #[test]
    fn user_text_blocks_concat() {
        let raw = r#"{"type":"user","message":{"content":[{"type":"text","text":"first"},{"type":"text","text":"second"},{"type":"image"}]}}"#;
        let out = decode(raw);
        assert_eq!(out[0].content, "first\nsecond");
    }

    #[test]
    fn local_command_user_skipped() {
        let raw = r#"{"type":"user","message":{"content":"<local-command-stdout>foo</local-command-stdout>"}}"#;
        assert!(decode(raw).is_empty());
    }

    #[test]
    fn assistant_text_and_tool_use() {
        let raw = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"running"},{"type":"tool_use","name":"bash","input":{"command":"ls"}}]}}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].role, "assistant");
        assert_eq!(out[0].content, "running");
        assert_eq!(out[1].role, "tool_use");
        assert_eq!(out[1].tool_name.as_deref(), Some("bash"));
        assert_eq!(out[1].content, "ls");
    }

    #[test]
    fn malformed_lines_skipped_not_propagated() {
        let raw = "not-json\n{\"type\":\"user\",\"message\":{\"content\":\"ok\"}}\n";
        let out = decode(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "ok");
    }

    #[test]
    fn tool_summary_truncated_at_cap() {
        let big = "x".repeat(MAX_TOOL_SUMMARY_BYTES + 100);
        let raw = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"bash","input":{{"command":"{big}"}}}}]}}}}"#
        );
        let out = decode(&raw);
        assert_eq!(out.len(), 1);
        assert!(out[0].content.len() <= MAX_TOOL_SUMMARY_BYTES + 4);
        assert!(out[0].content.ends_with('…'));
    }

    #[test]
    fn truncate_summary_never_splits_a_multibyte_char() {
        let s = "é".repeat(MAX_TOOL_SUMMARY_BYTES);
        let out = truncate_summary(&s);
        assert!(out.ends_with('…'));
        let body = out.strip_suffix('…').expect("ellipsis appended");
        assert!(body.chars().all(|c| c == 'é'), "no split character");
        assert!(body.len() <= MAX_TOOL_SUMMARY_BYTES);

        let s = "你".repeat(MAX_TOOL_SUMMARY_BYTES);
        let out = truncate_summary(&s);
        let body = out.strip_suffix('…').expect("ellipsis appended");
        assert_eq!(body.len() % 3, 0, "must land on a 3-byte char boundary");
        assert!(body.len() <= MAX_TOOL_SUMMARY_BYTES);
    }

    #[test]
    fn truncate_summary_leaves_short_input_untouched() {
        assert_eq!(truncate_summary("你好"), "你好");
    }
}
