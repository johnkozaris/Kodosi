use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::headless_host) enum DeviceRpcResponse {
    Revoked {
        revoked_device_id: String,
        new_generation: u64,
    },
    LinkApproved {
        approved_user_code: String,
        approved_device_id: String,
        approved_device_label: String,
        new_generation: u64,
    },
    SelfLinkStarted {
        device_id: String,
        user_code: String,
        expires_at: String,
    },
    SelfLinkCancellationRequested,
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::DeviceRpcResponse;

    #[test]
    fn cancellation_response_is_nonterminal_acknowledgement() {
        let value = serde_json::to_value(DeviceRpcResponse::SelfLinkCancellationRequested)
            .expect("serialize cancellation acknowledgement");

        assert_eq!(
            value,
            serde_json::json!({ "type": "self_link_cancellation_requested" })
        );
        let round: DeviceRpcResponse =
            serde_json::from_value(value).expect("deserialize cancellation acknowledgement");
        assert_eq!(round, DeviceRpcResponse::SelfLinkCancellationRequested);
    }
}
