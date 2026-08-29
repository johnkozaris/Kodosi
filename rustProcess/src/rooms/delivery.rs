use crate::agent_intel::telemetry::ExtensionObservation;
use crate::host_protocol::RoomAgentDeliveryState;

pub(crate) struct DeliveryTransition {
    pub(crate) state: RoomAgentDeliveryState,
    pub(crate) event_id: Option<String>,
    pub(crate) detail: Option<String>,
}

pub(crate) fn transition_from_extension(
    observation: &ExtensionObservation,
) -> Option<DeliveryTransition> {
    let event_id = observation
        .data
        .get("eventId")
        .and_then(serde_json::Value::as_str)
        .map(|value| value.chars().take(240).collect());
    match observation.event_type.as_str() {
        "kodosi.channel.ready" => Some(DeliveryTransition {
            state: RoomAgentDeliveryState::Ready,
            event_id: None,
            detail: Some("Session-scoped Claude Channel connected".to_owned()),
        }),
        "kodosi.extension.ready" => Some(DeliveryTransition {
            state: RoomAgentDeliveryState::Ready,
            event_id: None,
            detail: Some("Session-scoped Copilot extension connected".to_owned()),
        }),
        "kodosi.room.offered" => Some(DeliveryTransition {
            state: RoomAgentDeliveryState::Offered,
            event_id,
            detail: None,
        }),
        "kodosi.room.accepted" => Some(DeliveryTransition {
            state: RoomAgentDeliveryState::AcceptedByTransport,
            event_id,
            detail: None,
        }),
        "kodosi.room.acted" => Some(DeliveryTransition {
            state: RoomAgentDeliveryState::ActedOn,
            event_id,
            detail: None,
        }),
        "kodosi.room.delivery_failed" => Some(DeliveryTransition {
            state: RoomAgentDeliveryState::Failed,
            event_id,
            detail: observation
                .data
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(|value| value.chars().take(240).collect()),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_channel_ready_is_a_session_scoped_delivery_signal() {
        let ready = transition_from_extension(&ExtensionObservation {
            session_id: "session".to_owned(),
            event_type: "kodosi.channel.ready".to_owned(),
            timestamp: "now".to_owned(),
            data: serde_json::json!({}),
        })
        .expect("ready");

        assert_eq!(ready.state, RoomAgentDeliveryState::Ready);
        assert_eq!(
            ready.detail.as_deref(),
            Some("Session-scoped Claude Channel connected")
        );
    }

    #[test]
    fn transport_acceptance_is_distinct_from_agent_action() {
        let accepted = transition_from_extension(&ExtensionObservation {
            session_id: "session".to_owned(),
            event_type: "kodosi.room.accepted".to_owned(),
            timestamp: "now".to_owned(),
            data: serde_json::json!({"eventId":"chat:room:1"}),
        })
        .expect("accepted");
        let acted = transition_from_extension(&ExtensionObservation {
            session_id: "session".to_owned(),
            event_type: "kodosi.room.acted".to_owned(),
            timestamp: "now".to_owned(),
            data: serde_json::json!({"eventId":"chat:room:1"}),
        })
        .expect("acted");
        assert_eq!(accepted.state, RoomAgentDeliveryState::AcceptedByTransport);
        assert_eq!(acted.state, RoomAgentDeliveryState::ActedOn);
    }
}
