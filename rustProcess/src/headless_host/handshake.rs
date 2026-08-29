use serde::{Deserialize, Serialize};

pub(in crate::headless_host) const HOST_PROTOCOL_VERSION: u8 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::headless_host) enum TerminalLaneCapability {
    Write,
    ReadOnly,
}

impl TerminalLaneCapability {
    pub(in crate::headless_host) const fn can_write(self) -> bool {
        matches!(self, Self::Write)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::headless_host) enum ConnectionLane {
    #[default]
    Control,
    Terminal {
        session_id: String,
        #[serde(default)]
        capture: bool,
    },
    DeviceRevoke {
        expected_account_user_id: String,
        device_id: String,
    },
    DeviceApproveLink {
        expected_account_user_id: String,
        user_code: String,
    },
    DeviceStartSelfLink {
        expected_account_user_id: String,
        #[serde(default)]
        label: Option<String>,
    },
    DeviceCancelSelfLink {
        expected_account_user_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::headless_host) enum ControlClientFrame {
    Snapshot { refresh_id: uuid::Uuid },
    Command { command: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::headless_host) enum ControlServerFrame {
    Event {
        event: Box<crate::HostEvent>,
    },
    Snapshot {
        refresh_id: uuid::Uuid,
        account_user_id: Option<String>,
        account_epoch: u64,
        remote_operations_ready: bool,
        auth: crate::AuthEvent,
        sessions: Vec<crate::SessionListEntry>,
        rooms: Vec<crate::RoomListEntry>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::headless_host) struct HostHelloRequest {
    pub(in crate::headless_host) token: String,
    pub(in crate::headless_host) protocol_version: u8,
    #[serde(default)]
    pub(in crate::headless_host) lane: ConnectionLane,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::headless_host) struct HostHelloResponse {
    pub(in crate::headless_host) accepted: bool,
    pub(in crate::headless_host) server_version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::headless_host) message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::headless_host) capability: Option<TerminalLaneCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::headless_host) account_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::headless_host) account_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::headless_host) remote_operations_ready: Option<bool>,
}

impl HostHelloResponse {
    pub(in crate::headless_host) fn accept() -> Self {
        Self {
            accepted: true,
            server_version: HOST_PROTOCOL_VERSION,
            message: None,
            capability: None,
            account_user_id: None,
            account_epoch: None,
            remote_operations_ready: None,
        }
    }

    pub(in crate::headless_host) fn accept_control(status: crate::RemoteCommandStatus) -> Self {
        Self {
            accepted: true,
            server_version: HOST_PROTOCOL_VERSION,
            message: None,
            capability: None,
            account_epoch: Some(status.account_epoch),
            account_user_id: status.account_user_id,
            remote_operations_ready: Some(status.remote_operations_ready),
        }
    }

    pub(in crate::headless_host) fn accept_terminal(capability: TerminalLaneCapability) -> Self {
        Self {
            accepted: true,
            server_version: HOST_PROTOCOL_VERSION,
            message: None,
            capability: Some(capability),
            account_user_id: None,
            account_epoch: None,
            remote_operations_ready: None,
        }
    }

    pub(in crate::headless_host) fn reject(message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            server_version: HOST_PROTOCOL_VERSION,
            message: Some(message.into()),
            capability: None,
            account_user_id: None,
            account_epoch: None,
            remote_operations_ready: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConnectionLane, ControlClientFrame, ControlServerFrame, HOST_PROTOCOL_VERSION,
        HostHelloRequest, HostHelloResponse, TerminalLaneCapability,
    };

    #[test]
    fn cancellation_acknowledgement_requires_headless_protocol_v12() {
        assert_eq!(HOST_PROTOCOL_VERSION, 12);
    }

    #[test]
    fn snapshot_control_frames_round_trip_exact_uuid() {
        let refresh_id = uuid::Uuid::now_v7();
        let request = serde_json::to_value(ControlClientFrame::Snapshot { refresh_id })
            .expect("snapshot request");
        assert_eq!(request["type"], "snapshot");
        assert_eq!(request["refresh_id"], refresh_id.to_string());

        let response = ControlServerFrame::Snapshot {
            refresh_id,
            account_user_id: None,
            account_epoch: 4,
            remote_operations_ready: false,
            auth: crate::AuthEvent::Required {
                reason: crate::AuthRequiredReason::SignedOut,
                account_epoch: 4,
            },
            sessions: Vec::new(),
            rooms: Vec::new(),
        };
        let round: ControlServerFrame =
            serde_json::from_value(serde_json::to_value(response).expect("snapshot response"))
                .expect("round trip response");
        assert!(matches!(
            round,
            ControlServerFrame::Snapshot {
                refresh_id: actual,
                account_epoch: 4,
                remote_operations_ready: false,
                ..
            } if actual == refresh_id
        ));
    }

    #[test]
    fn hello_request_rejects_unknown_fields() {
        let error = serde_json::from_str::<HostHelloRequest>(
            r#"{"token":"t","protocol_version":1,"unexpected":true}"#,
        )
        .expect_err("unknown hello fields must fail closed");

        assert!(error.to_string().contains("unexpected"));
    }

    #[test]
    fn terminal_lane_rejects_unknown_fields() {
        let error = serde_json::from_str::<HostHelloRequest>(
            r#"{"token":"t","protocol_version":1,"lane":{"type":"terminal","session_id":"s","capture":true,"extra":1}}"#,
        )
        .expect_err("unknown terminal lane fields must fail closed");

        assert!(error.to_string().contains("extra"));
    }

    #[test]
    fn hello_response_rejects_unknown_fields() {
        let error = serde_json::from_str::<HostHelloResponse>(
            r#"{"accepted":true,"server_version":1,"unexpected":true}"#,
        )
        .expect_err("unknown hello response fields must fail closed");

        assert!(error.to_string().contains("unexpected"));
    }

    #[test]
    fn control_accept_carries_exact_runtime_authority_status() {
        let value = serde_json::to_value(HostHelloResponse::accept_control(
            crate::RemoteCommandStatus {
                account_user_id: Some("01900000-0000-7000-8000-000000000001".to_owned()),
                account_epoch: 1,
                remote_operations_ready: false,
            },
        ))
        .expect("control hello");

        assert_eq!(
            value["account_user_id"],
            "01900000-0000-7000-8000-000000000001"
        );
        assert_eq!(value["account_epoch"], 1);
        assert_eq!(value["remote_operations_ready"], false);
    }

    #[test]
    fn device_rpc_lanes_require_headless_v7() {
        for lane in [
            ConnectionLane::DeviceRevoke {
                expected_account_user_id: "user-1".to_owned(),
                device_id: "device-1".to_owned(),
            },
            ConnectionLane::DeviceApproveLink {
                expected_account_user_id: "user-1".to_owned(),
                user_code: "ABCD-EFGH".to_owned(),
            },
            ConnectionLane::DeviceStartSelfLink {
                expected_account_user_id: "user-1".to_owned(),
                label: Some("Mac".to_owned()),
            },
            ConnectionLane::DeviceCancelSelfLink {
                expected_account_user_id: "user-1".to_owned(),
            },
        ] {
            let request = HostHelloRequest {
                token: "token".to_owned(),
                protocol_version: HOST_PROTOCOL_VERSION,
                lane: lane.clone(),
            };
            let round_trip: HostHelloRequest = serde_json::from_value(
                serde_json::to_value(request).expect("serialize device lane"),
            )
            .expect("deserialize device lane");
            assert_eq!(round_trip.lane, lane);
        }
    }

    #[test]
    fn terminal_accept_declares_negotiated_capability() {
        let value = serde_json::to_value(HostHelloResponse::accept_terminal(
            TerminalLaneCapability::ReadOnly,
        ))
        .unwrap_or_else(|error| panic!("terminal hello should serialize: {error}"));

        assert_eq!(
            value.get("capability").and_then(serde_json::Value::as_str),
            Some("read_only")
        );
    }
}
