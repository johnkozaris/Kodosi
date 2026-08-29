use super::{
    host_relay::registry::HostRelayRegistry, shared_session_registry::SharedSessionRegistry,
};

#[derive(Debug, Default)]
pub(crate) struct SharingState {
    pub(crate) host_relays: HostRelayRegistry,
    pub(crate) shared_sessions: SharedSessionRegistry,
}
