use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HitlPromptType {
    AllowDeny,
    Enter,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HitlPrompt {
    pub prompt_type: HitlPromptType,
    pub tool_name: Option<String>,
    pub description: Option<String>,
}
