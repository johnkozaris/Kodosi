use std::collections::HashSet;

use serde_json::Value;

use super::adapter::CURRENT_ADAPTER;
use crate::transcript::{ConversationEntry, DecodeError, TranscriptDecoder};

const MAX_TOOL_SUMMARY_BYTES: usize = 1024;

const SAFE_ARG_KEYS: &[&str] = &[
    "command",
    "filePath",
    "file_path",
    "path",
    "description",
    "query",
    "intent",
];

#[derive(Debug, Default, Clone, Copy)]
pub struct CopilotTranscriptDecoder;

impl CopilotTranscriptDecoder {
    pub const INSTANCE: &'static Self = &Self;
}

impl TranscriptDecoder for CopilotTranscriptDecoder {
    fn decode_conversation(&self, raw: &str) -> Result<Vec<ConversationEntry>, DecodeError> {
        let mut entries = Vec::new();
        let mut emitted_tool_call_ids: HashSet<String> = HashSet::new();

        for (index, framed_line) in raw.split_inclusive('\n').enumerate() {
            let terminated = framed_line.ends_with('\n');
            let without_newline = framed_line.strip_suffix('\n').unwrap_or(framed_line);
            let line = without_newline
                .strip_suffix('\r')
                .unwrap_or(without_newline);
            if line.trim().is_empty() {
                continue;
            }
            let event = match CURRENT_ADAPTER.decode_line(line) {
                Ok(event) => event,
                Err(_) if !terminated => break,
                Err(error) => {
                    return Err(DecodeError::Malformed(format!(
                        "Copilot V1 event line {}: {error}",
                        index + 1
                    )));
                }
            };

            match event.event_type.as_str() {
                "user.message" => {
                    decode_user_message(&event.data, event.timestamp, &mut entries);
                }
                "assistant.message" => {
                    decode_assistant_message(
                        &event.data,
                        event.timestamp.as_deref(),
                        &mut entries,
                        &mut emitted_tool_call_ids,
                    );
                }
                "tool.execution_start" => {
                    decode_tool_execution_start(
                        &event.data,
                        event.timestamp,
                        &mut entries,
                        &mut emitted_tool_call_ids,
                    );
                }
                _ => {}
            }
        }

        Ok(entries)
    }
}

fn decode_user_message(
    data: &Value,
    timestamp: Option<String>,
    entries: &mut Vec<ConversationEntry>,
) {
    let Some(content) = CURRENT_ADAPTER.user_message_content(data) else {
        return;
    };
    if content.is_empty() {
        return;
    }
    entries.push(ConversationEntry {
        role: "user".to_owned(),
        content: content.to_owned(),
        tool_name: None,
        timestamp,
    });
}

fn decode_assistant_message(
    data: &Value,
    timestamp: Option<&str>,
    entries: &mut Vec<ConversationEntry>,
    emitted_tool_call_ids: &mut HashSet<String>,
) {
    if let Some(content) = CURRENT_ADAPTER.assistant_message_content(data)
        && !content.is_empty()
    {
        entries.push(ConversationEntry {
            role: "assistant".to_owned(),
            content: content.to_owned(),
            tool_name: None,
            timestamp: timestamp.map(str::to_owned),
        });
    }

    let Some(requests) = CURRENT_ADAPTER.assistant_tool_requests(data) else {
        return;
    };
    for req in requests {
        let name = req.get("name").and_then(Value::as_str).unwrap_or("tool");
        let summary = req
            .get("arguments")
            .and_then(extract_arg_summary)
            .unwrap_or_default();

        entries.push(ConversationEntry {
            role: "tool_use".to_owned(),
            content: summary,
            tool_name: Some(name.to_owned()),
            timestamp: timestamp.map(str::to_owned),
        });

        if let Some(id) = req.get("toolCallId").and_then(Value::as_str) {
            emitted_tool_call_ids.insert(id.to_owned());
        }
    }
}

fn decode_tool_execution_start(
    data: &Value,
    timestamp: Option<String>,
    entries: &mut Vec<ConversationEntry>,
    emitted_tool_call_ids: &mut HashSet<String>,
) {
    let id = data.get("toolCallId").and_then(Value::as_str);
    if let Some(id) = id
        && !emitted_tool_call_ids.insert(id.to_owned())
    {
        return;
    }
    let name = data
        .get("toolName")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let summary = data
        .get("arguments")
        .and_then(extract_arg_summary)
        .unwrap_or_default();
    entries.push(ConversationEntry {
        role: "tool_use".to_owned(),
        content: summary,
        tool_name: Some(name.to_owned()),
        timestamp,
    });
}

fn extract_arg_summary(args: &Value) -> Option<String> {
    let obj = args.as_object()?;
    for key in SAFE_ARG_KEYS {
        if let Some(v) = obj.get(*key).and_then(Value::as_str)
            && !v.is_empty()
        {
            return Some(truncate_summary(&crate::jsonl::scrub_secret_patterns(v)));
        }
    }
    None
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
        let framed = if raw.ends_with('\n') {
            raw.to_owned()
        } else {
            format!("{raw}\n")
        };
        CopilotTranscriptDecoder
            .decode_conversation(&framed)
            .expect("copilot decoder never returns Err on conversation")
    }

    #[test]
    fn user_message_uses_content_not_transformed() {
        let raw = r#"{"type":"user.message","data":{"content":"hello","transformedContent":"<current_datetime>...</current_datetime>\n\nhello\n\n<reminder>...</reminder>"},"timestamp":"2026-04-15T22:04:09.933Z"}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content, "hello");
        assert!(!out[0].content.contains("reminder"));
        assert!(!out[0].content.contains("current_datetime"));
        assert_eq!(
            out[0].timestamp.as_deref(),
            Some("2026-04-15T22:04:09.933Z")
        );
    }

    #[test]
    fn assistant_message_emits_text_then_tool_uses() {
        let raw = r#"{"type":"assistant.message","data":{"content":"running","toolRequests":[{"toolCallId":"c1","name":"bash","arguments":{"command":"ls"}},{"toolCallId":"c2","name":"rg","arguments":{"query":"foo"}}]}}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, "assistant");
        assert_eq!(out[0].content, "running");
        assert_eq!(out[1].role, "tool_use");
        assert_eq!(out[1].tool_name.as_deref(), Some("bash"));
        assert_eq!(out[1].content, "ls");
        assert_eq!(out[2].role, "tool_use");
        assert_eq!(out[2].tool_name.as_deref(), Some("rg"));
        assert_eq!(out[2].content, "foo");
    }

    #[test]
    fn empty_assistant_content_skipped() {
        let raw = r#"{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"c1","name":"bash","arguments":{"command":"ls"}}]}}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "tool_use");
    }

    #[test]
    fn tool_execution_start_deduped_by_call_id() {
        let raw = r#"{"type":"assistant.message","data":{"content":"go","toolRequests":[{"toolCallId":"c1","name":"bash","arguments":{"command":"ls"}}]}}
{"type":"tool.execution_start","data":{"toolCallId":"c1","toolName":"bash","arguments":{"command":"ls"}}}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn tool_execution_start_emitted_when_unseen() {
        let raw = r#"{"type":"tool.execution_start","data":{"toolCallId":"orphan","toolName":"bash","arguments":{"command":"ls -la"}}}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "tool_use");
        assert_eq!(out[0].content, "ls -la");
    }

    #[test]
    fn raw_arguments_never_leak() {
        let raw = r#"{"type":"assistant.message","data":{"content":"go","toolRequests":[{"toolCallId":"c1","name":"bash","arguments":{"command":"ls","apiKey":"sk-live-abc","files":["secret.txt"],"environment":{"GITHUB_TOKEN":"gho_REAL"}}}]}}"#;
        let out = decode(raw);
        assert_eq!(out[1].role, "tool_use");
        assert_eq!(out[1].content, "ls");
        assert!(!out[1].content.contains("sk-live-abc"));
        assert!(!out[1].content.contains("gho_REAL"));
        assert!(!out[1].content.contains("secret.txt"));
    }

    #[test]
    fn allowlisted_value_redacts_embedded_secret_patterns() {
        let raw = "{\"type\":\"assistant.message\",\"data\":{\"content\":\"go\",\"toolRequests\":[{\"toolCallId\":\"c1\",\"name\":\"bash\",\"arguments\":{\"command\":\"curl -H 'Authorization: gho_LEAKxxxxxxxxxx' https://api\"}},{\"toolCallId\":\"c2\",\"name\":\"bash\",\"arguments\":{\"command\":\"GITHUB_TOKEN=gho_xxxxxxxxxxxx OPENAI_API_KEY=sk-live-zzzzzzz make\"}},{\"toolCallId\":\"c3\",\"name\":\"bash\",\"arguments\":{\"description\":\"Use the sk-ant-yyyyyyyyy key to call Anthropic\"}}]}}";
        let out = decode(raw);
        let tool_rows: Vec<&str> = out
            .iter()
            .filter(|e| e.role == "tool_use")
            .map(|e| e.content.as_str())
            .collect();
        assert_eq!(tool_rows.len(), 3);
        for row in &tool_rows {
            assert!(!row.contains("gho_LEAK"), "gho_LEAK leaked: {row}");
            assert!(!row.contains("gho_xxxxxxxxxxxx"), "gho_x leaked: {row}");
            assert!(!row.contains("sk-live-zzzzzzz"), "sk-live leaked: {row}");
            assert!(!row.contains("sk-ant-yyyyyyyyy"), "sk-ant leaked: {row}");
            assert!(row.contains("<redacted>"), "no scrub marker: {row}");
        }
        assert!(tool_rows[0].contains("curl"));
        assert!(tool_rows[1].contains("make"));
        assert!(tool_rows[2].contains("Anthropic"));
    }

    #[test]
    fn scrub_pass_through_for_benign_strings() {
        let cases = ["ls -la", "cd /tmp && cargo build", "find . -name '*.rs'"];
        for case in cases {
            assert_eq!(crate::jsonl::scrub_secret_patterns(case), case);
        }
    }

    #[test]
    fn unknown_event_types_skipped_silently() {
        let raw = r#"{"type":"session.start","data":{}}
{"type":"hook.start","data":{}}
{"type":"subagent.started","data":{}}
{"type":"system.notification","data":{}}
{"type":"user.message","data":{"content":"hi"}}"#;
        let out = decode(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "hi");
    }

    #[test]
    fn malformed_complete_jsonl_line_returns_decode_error() {
        let raw = "garbage\n{\"type\":\"user.message\",\"data\":{\"content\":\"ok\"}}\n";
        let error = CopilotTranscriptDecoder
            .decode_conversation(raw)
            .expect_err("malformed complete records must be visible");
        assert!(
            error.to_string().contains("line 1"),
            "error must identify the rejected record: {error}"
        );
    }

    #[test]
    fn unterminated_final_partial_line_is_ignored_until_next_read() {
        let raw = concat!(
            "{\"type\":\"user.message\",\"data\":{\"content\":\"complete\"}}\n",
            "{\"type\":\"assistant.message\",\"data\":{\"content\":\"partial"
        );
        let out = CopilotTranscriptDecoder
            .decode_conversation(raw)
            .expect("partial final line is retryable");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "complete");
    }

    #[test]
    fn valid_unterminated_final_record_is_projected() {
        let out = CopilotTranscriptDecoder
            .decode_conversation(
                "{\"type\":\"user.message\",\"data\":{\"content\":\"complete without newline\"}}",
            )
            .expect("valid final record");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "complete without newline");
    }

    #[test]
    fn newline_terminated_malformed_record_is_not_treated_as_partial() {
        let error = CopilotTranscriptDecoder
            .decode_conversation("not-json\n")
            .expect_err("terminated malformed record must fail");
        assert!(error.to_string().contains("line 1"));
    }

    #[test]
    fn checked_fixture_uses_the_same_v1_projection_as_live_events() {
        let raw = include_str!("../../tests/fixtures/copilot_cli_session.jsonl");
        let out = decode(raw);
        let user = out
            .iter()
            .find(|entry| entry.role == "user")
            .expect("fixture user message projects");
        assert_eq!(user.content, "Add a health check endpoint.");
    }

    #[test]
    fn tool_summary_truncated_at_cap() {
        let big = "x".repeat(MAX_TOOL_SUMMARY_BYTES + 100);
        let raw = format!(
            r#"{{"type":"assistant.message","data":{{"toolRequests":[{{"toolCallId":"c1","name":"bash","arguments":{{"command":"{big}"}}}}]}}}}"#
        );
        let out = decode(&raw);
        assert_eq!(out.len(), 1);
        assert!(out[0].content.len() <= MAX_TOOL_SUMMARY_BYTES + 4);
        assert!(out[0].content.ends_with('…'));
    }

    #[test]
    fn truncate_summary_never_splits_a_multibyte_char() {
        let s = "你".repeat(MAX_TOOL_SUMMARY_BYTES);
        let out = truncate_summary(&s);
        let body = out.strip_suffix('…').expect("ellipsis appended");
        assert_eq!(body.len() % 3, 0, "must land on a 3-byte char boundary");
        assert!(body.len() <= MAX_TOOL_SUMMARY_BYTES);
        assert!(body.chars().all(|c| c == '你'));
    }

    #[test]
    fn truncate_summary_leaves_short_input_untouched() {
        assert_eq!(truncate_summary("你好"), "你好");
    }
}
