use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationEntry {
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationPage {
    pub entries: Vec<ConversationEntry>,
    pub next_before_byte: Option<u64>,
    pub source_file_bytes: u64,
    pub read_bytes: u64,
    pub source_records: usize,
    pub degraded_reason: Option<String>,
}

#[derive(Debug)]
pub enum DecodeError {
    Malformed(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(msg) => write!(f, "malformed transcript: {msg}"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub trait TranscriptDecoder: Send + Sync {
    fn decode_conversation(&self, raw: &str) -> Result<Vec<ConversationEntry>, DecodeError>;
}
