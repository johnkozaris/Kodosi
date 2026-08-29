use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstructionSource {
    pub path: String,
    pub scope: InstructionScope,
    pub order: u8,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum InstructionScope {
    User,
    Project,
    Local,
    Memory,
    Session,
}
