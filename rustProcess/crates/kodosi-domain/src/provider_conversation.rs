use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ProviderConversationProvider {
    Claude,
    Copilot,
}

impl ProviderConversationProvider {
    pub const fn executable(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConversationIdentity {
    pub provider: ProviderConversationProvider,
    pub native_conversation_id: String,
}
