use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotEventAdapter {
    V1,
}

pub const CURRENT_ADAPTER: CopilotEventAdapter = CopilotEventAdapter::V1;

#[derive(Debug, Clone)]
pub struct CopilotEvent {
    pub event_type: String,
    pub timestamp: Option<String>,
    pub data: Value,
    pub non_empty_envelope: bool,
}

#[derive(Debug, Error)]
pub enum CopilotAdapterError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("event envelope must be a JSON object")]
    Envelope,
}

impl CopilotEventAdapter {
    pub fn decode_line(self, line: &str) -> Result<CopilotEvent, CopilotAdapterError> {
        let value = serde_json::from_str::<Value>(line)?;
        let object = value.as_object().ok_or(CopilotAdapterError::Envelope)?;
        let event_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let timestamp = object
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let data = object.get("data").cloned().unwrap_or(Value::Null);
        Ok(CopilotEvent {
            event_type,
            timestamp,
            data,
            non_empty_envelope: !object.is_empty(),
        })
    }

    #[must_use]
    pub fn user_message_content(self, data: &Value) -> Option<&str> {
        match self {
            Self::V1 => data.get("content").and_then(Value::as_str),
        }
    }

    #[must_use]
    pub fn assistant_message_content(self, data: &Value) -> Option<&str> {
        match self {
            Self::V1 => data.get("content").and_then(Value::as_str),
        }
    }

    #[must_use]
    pub fn assistant_tool_requests(self, data: &Value) -> Option<&[Value]> {
        match self {
            Self::V1 => data
                .get("toolRequests")
                .and_then(Value::as_array)
                .map(Vec::as_slice),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_pins_current_user_message_field() {
        let data = serde_json::json!({
            "content": "canonical",
            "text": "legacy-wrong-field"
        });
        assert_eq!(
            CURRENT_ADAPTER.user_message_content(&data),
            Some("canonical")
        );
    }

    #[test]
    fn unknown_object_fields_survive_adapter_decode() {
        let event = CURRENT_ADAPTER
            .decode_line(
                r#"{"type":"user.message","data":{"content":"hi","future":{"kept":true}}}"#,
            )
            .expect("valid object");
        assert_eq!(event.data["future"]["kept"], true);
    }

    #[test]
    fn non_object_complete_record_is_adapter_error() {
        std::assert_matches!(
            CURRENT_ADAPTER.decode_line("[1,2,3]"),
            Err(CopilotAdapterError::Envelope)
        );
    }
}
