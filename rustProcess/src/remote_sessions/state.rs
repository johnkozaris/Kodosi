use super::relay::registry::SessionRelayRegistry;

#[derive(Debug, Default)]
pub(crate) struct RemoteSessionsState {
    pub(crate) session_relays: SessionRelayRegistry,
}
