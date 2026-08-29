use std::path::PathBuf;

use crate::claude::ClaudeCodeProvider;
use crate::copilot::CopilotCliProvider;

#[allow(
    clippy::large_enum_variant,
    reason = "The whole point of this enum is to avoid the Box<dyn AgentFileSystem> \
              indirection. Boxing one variant would just put the heap allocation back."
)]
pub enum AgentFs {
    Claude(ClaudeCodeProvider),
    Copilot(CopilotCliProvider),
}

impl AgentFs {
    #[must_use]
    pub fn from_kind(kind: kodosi_session::AgentKind) -> Option<Self> {
        use kodosi_session::AgentKind;
        match kind {
            AgentKind::Claude => Some(Self::Claude(ClaudeCodeProvider::new())),
            AgentKind::Copilot => Some(Self::Copilot(CopilotCliProvider::new())),
        }
    }

    #[must_use]
    pub fn transcript_path(&self, cwd: &str, session_id: &str) -> Option<PathBuf> {
        match self {
            Self::Claude(p) => p.transcript_path(cwd, session_id),
            Self::Copilot(p) => p.transcript_path(cwd, session_id),
        }
    }

    #[must_use]
    pub fn transcript_root(&self) -> Option<PathBuf> {
        match self {
            Self::Claude(p) => p.transcript_root(),
            Self::Copilot(p) => p.transcript_root(),
        }
    }

    #[must_use]
    pub fn transcript_decoder(&self) -> &'static dyn crate::TranscriptDecoder {
        match self {
            Self::Claude(p) => p.transcript_decoder(),
            Self::Copilot(p) => p.transcript_decoder(),
        }
    }
}
