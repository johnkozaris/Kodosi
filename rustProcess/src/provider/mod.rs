mod catalog;
mod config;
mod decode;
mod history;
mod read;
mod resume;
mod storage;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Copilot,
}

impl Provider {
    pub const fn executable(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationIdentity {
    pub provider: Provider,
    pub native_conversation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRef {
    pub provider: Provider,
    pub native_conversation_id: String,
    pub working_directory: String,
    pub title: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationListPage {
    pub items: Vec<ConversationRef>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub response_bytes: usize,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub provider: Provider,
    pub executable: Option<String>,
    pub files: Vec<ConfigFile>,
    pub message: Option<String>,
}

pub async fn discover(
    home: &Path,
    provider: Provider,
    working_directory: &str,
    cursor: Option<&str>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationListPage, String> {
    let home = home.to_owned();
    let directory = storage::canonical_directory(working_directory)?;
    let cursor = cursor.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        catalog::discover(
            &home,
            provider,
            &directory,
            cursor.as_deref(),
            limit,
            max_bytes,
        )
    })
    .await
    .map_err(|error| format!("Conversation discovery failed: {error}"))?
}

pub async fn discover_history(
    home: &Path,
    provider: Provider,
    directory: Option<&str>,
    cursor: Option<&str>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationListPage, String> {
    if let Some(directory) = directory {
        return discover(home, provider, directory, cursor, limit, max_bytes).await;
    }
    let home = home.to_owned();
    let cursor = cursor.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        history::discover(&home, provider, cursor.as_deref(), limit, max_bytes)
    })
    .await
    .map_err(|error| format!("History discovery failed: {error}"))?
}

pub async fn read(
    home: &Path,
    provider: Provider,
    working_directory: &str,
    native_conversation_id: &str,
    before_byte: Option<u64>,
    limit: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    let home = home.to_owned();
    let directory = storage::canonical_directory(working_directory)?;
    let id = storage::conversation_id(native_conversation_id)?;
    tokio::task::spawn_blocking(move || {
        let file = storage::conversation_file(&home, provider, &directory, &id)?;
        read::page(file, provider, before_byte, limit, max_bytes)
    })
    .await
    .map_err(|error| format!("Conversation read failed: {error}"))?
}

pub async fn validate_resume(
    home: &Path,
    provider: Provider,
    working_directory: &str,
    native_conversation_id: &str,
) -> Result<PathBuf, String> {
    let home = home.to_owned();
    let directory = storage::canonical_directory(working_directory)?;
    let id = storage::conversation_id(native_conversation_id)?;
    tokio::task::spawn_blocking(move || {
        let file = storage::conversation_file(&home, provider, &directory, &id)?;
        resume::ensure_inactive(&home, provider, &directory, &id, file)?;
        Ok(directory)
    })
    .await
    .map_err(|error| format!("Resume validation failed: {error}"))?
}

pub async fn inspect(
    home: &Path,
    provider: Provider,
    working_directory: Option<&str>,
) -> Result<ProviderInfo, String> {
    let home = home.to_owned();
    let directory = working_directory
        .map(storage::canonical_directory)
        .transpose()?;
    tokio::task::spawn_blocking(move || config::inspect(&home, provider, directory.as_deref()))
        .await
        .map_err(|error| format!("Provider configuration inspection failed: {error}"))?
}

pub fn resolve_executable(provider: Provider) -> Option<PathBuf> {
    config::resolve_executable(provider)
}

#[cfg(test)]
mod tests;
