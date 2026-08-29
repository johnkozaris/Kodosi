use std::sync::Arc;

use kodosi_session::AgentKind;

use crate::claude::ClaudeLiveAgentProvider;
use crate::copilot::CopilotLiveAgentProvider;
use crate::domain::ids::SessionId;
use crate::ports::live_provider::LiveAgentProvider;

#[must_use]
pub fn create_live_provider(
    agent_name: &str,
    cwd: &str,
    session_id: &SessionId,
) -> Option<Arc<dyn LiveAgentProvider>> {
    match AgentKind::from_banner(agent_name)? {
        AgentKind::Claude => Some(Arc::new(ClaudeLiveAgentProvider::new(session_id, cwd))),
        AgentKind::Copilot => Some(Arc::new(CopilotLiveAgentProvider::new(session_id, cwd))),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code may panic on unexpected failures"
)]
mod tests {
    use super::create_live_provider;
    use crate::domain::ids::SessionId;

    #[test]
    fn unknown_banner_returns_none() {
        let p = create_live_provider("not-an-agent", "/tmp", &SessionId::new("sid"));
        assert!(p.is_none());
    }

    #[test]
    fn known_banners_create_providers() {
        assert!(
            create_live_provider("Claude", "/workspace", &SessionId::new("sid-claude")).is_some()
        );
        assert!(
            create_live_provider(
                "GitHub Copilot",
                "/workspace",
                &SessionId::new("sid-copilot"),
            )
            .is_some()
        );
    }
}
