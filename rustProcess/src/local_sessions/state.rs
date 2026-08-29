use kodosi_domain::terminal::TerminalSize;

use super::runtime_registry::OwnedSessionRuntimeRegistry;
use crate::session_runtime::registry::SessionRegistry;

#[derive(Debug)]
pub(crate) struct LocalSessionsState {
    pub(crate) sessions: SessionRegistry,
    pub(crate) owned_session_runtimes: OwnedSessionRuntimeRegistry,
    pub(crate) last_terminal_size: TerminalSize,
    pub(crate) next_session_number: usize,
}

impl Default for LocalSessionsState {
    fn default() -> Self {
        Self {
            sessions: SessionRegistry::default(),
            owned_session_runtimes: OwnedSessionRuntimeRegistry::default(),
            last_terminal_size: TerminalSize::default(),
            next_session_number: 1,
        }
    }
}
