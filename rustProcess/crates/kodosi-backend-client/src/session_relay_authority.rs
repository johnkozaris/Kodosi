use std::{collections::BTreeMap, sync::OnceLock};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionRelayAuthority {
    pub relay_protocol_version: u32,
    pub compatibility_policy: String,
    pub messages: BTreeMap<String, RelayMessageAuthority>,
    #[cfg(test)]
    pub close_reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelayMessageAuthority {
    pub direction: String,
    pub framing: String,
    #[cfg(test)]
    pub required_fields: Vec<String>,
    #[serde(default)]
    #[cfg(test)]
    pub optional_fields: Vec<String>,
    pub max_bytes: usize,
}

pub(crate) fn session_relay_authority() -> &'static SessionRelayAuthority {
    static AUTHORITY: OnceLock<SessionRelayAuthority> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../protocol/session-relay-authority.json"
        )))
        .unwrap_or_else(|error| panic!("session-relay-authority.json should deserialize: {error}"))
    })
}

pub(crate) fn relay_message_limit(message_type: &str) -> usize {
    session_relay_authority()
        .messages
        .get(message_type)
        .map_or(0, |spec| spec.max_bytes)
}

pub(crate) fn participant_outbound_message_limit(message_type: &str) -> Option<usize> {
    session_relay_authority()
        .messages
        .get(message_type)
        .filter(|spec| spec.framing == "json" && spec.direction == "participantToBackend")
        .map(|spec| spec.max_bytes)
}

pub(crate) fn participant_text_message_limit(message_type: &str) -> Option<usize> {
    session_relay_authority()
        .messages
        .get(message_type)
        .filter(|spec| {
            spec.framing == "json"
                && matches!(
                    spec.direction.as_str(),
                    "backendToParticipant" | "hostToBackendToParticipant"
                )
        })
        .map(|spec| spec.max_bytes)
}

pub(crate) fn participant_text_envelope_max_bytes() -> usize {
    session_relay_authority()
        .messages
        .values()
        .filter(|spec| {
            spec.framing == "json"
                && matches!(
                    spec.direction.as_str(),
                    "backendToParticipant" | "hostToBackendToParticipant"
                )
        })
        .map(|spec| spec.max_bytes)
        .max()
        .unwrap_or(0)
}

pub(crate) fn relay_protocol_version() -> u32 {
    session_relay_authority().relay_protocol_version
}

pub(crate) fn relay_protocol_uses_exact_match() -> bool {
    session_relay_authority().compatibility_policy == "exactMatch"
}

#[cfg(test)]
pub(crate) fn load_session_relay_authority() -> &'static SessionRelayAuthority {
    session_relay_authority()
}
