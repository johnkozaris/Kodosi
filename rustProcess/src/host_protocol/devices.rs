use kodosi_domain::device_link::{
    DeviceLinkOutcome, SelfDeviceLinkOutcome, is_canonical_user_code, normalize_user_code,
};
use serde::{Deserialize, Serialize};

use super::HostCommandValidationError;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum DeviceCommand {
    #[serde(rename = "devices.refresh")]
    Refresh,
    #[serde(rename = "devices.revoke")]
    Revoke {
        #[serde(rename = "deviceId")]
        device_id: String,
    },
    #[serde(rename = "devices.link.approve")]
    LinkApprove {
        #[serde(rename = "userCode")]
        user_code: String,
    },
    #[serde(rename = "devices.link.startSelf")]
    LinkStartSelf,
    #[serde(rename = "devices.link.cancelSelf")]
    LinkCancelSelf,
}

impl DeviceCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Revoke { .. } => "revoke",
            Self::LinkApprove { .. } => "link.approve",
            Self::LinkStartSelf => "link.startSelf",
            Self::LinkCancelSelf => "link.cancelSelf",
        }
    }

    pub(crate) fn validate(&self) -> Result<(), HostCommandValidationError> {
        match self {
            Self::Refresh | Self::LinkStartSelf | Self::LinkCancelSelf => Ok(()),
            Self::Revoke { device_id } => validate_present(device_id, "deviceId"),
            Self::LinkApprove { user_code } => {
                validate_present(user_code, "userCode")?;
                if is_canonical_user_code(&normalize_user_code(user_code)) {
                    Ok(())
                } else {
                    Err(HostCommandValidationError::InvalidField {
                        field: "userCode",
                        reason: "expected 8 backend-code characters in two groups of four, like BCDF-2345",
                    })
                }
            }
        }
    }

    pub(crate) fn user_code_for_error(&self) -> Option<String> {
        match self {
            Self::LinkApprove { user_code } => Some(normalize_user_code(user_code)),
            _ => None,
        }
    }
}

fn validate_present(value: &str, field: &'static str) -> Result<(), HostCommandValidationError> {
    if value.trim().is_empty() {
        return Err(HostCommandValidationError::EmptyField(field));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DeviceEvent {
    #[serde(rename = "devices.list")]
    List {
        #[serde(rename = "selfDeviceId")]
        self_device_id: String,
        #[serde(rename = "localDeviceEnrolled")]
        local_device_enrolled: bool,
        devices: Vec<MyDeviceEntry>,
    },
    #[serde(rename = "devices.link.snapshot")]
    LinkSnapshot {
        requests: Vec<DeviceLinkRequestEntry>,
    },
    #[serde(rename = "devices.link.requested")]
    LinkRequested {
        #[serde(rename = "userCode")]
        user_code: String,
        #[serde(rename = "deviceLabel")]
        device_label: String,
        #[serde(rename = "expiresAt")]
        expires_at: String,
    },
    #[serde(rename = "devices.link.resolved")]
    LinkResolved {
        #[serde(rename = "userCode")]
        user_code: String,
        outcome: DeviceLinkOutcome,
    },
    #[serde(rename = "devices.link.selfPending")]
    LinkSelfPending {
        #[serde(rename = "userCode")]
        user_code: String,
        #[serde(rename = "expiresAt")]
        expires_at: String,
    },
    #[serde(rename = "devices.link.selfResolved")]
    LinkSelfResolved { outcome: SelfDeviceLinkOutcome },
    #[serde(rename = "devices.error")]
    Error {
        #[serde(rename = "userCode", default, skip_serializing_if = "Option::is_none")]
        user_code: Option<String>,
        operation: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkRequestEntry {
    pub user_code: String,
    pub device_label: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MyDeviceEntry {
    pub device_id: String,
    pub label: String,
    pub cert_signer_device_id: String,
    pub cert_issued_at_ms: u64,
}

#[cfg(test)]
mod tests {
    use crate::host_protocol::HostCommandValidationError;

    use super::{
        DeviceCommand, DeviceEvent, DeviceLinkOutcome, MyDeviceEntry, SelfDeviceLinkOutcome,
    };

    #[test]
    fn serializes_device_list_shape() {
        let payload = serde_json::to_value(DeviceEvent::List {
            self_device_id: "device-self".to_owned(),
            local_device_enrolled: true,
            devices: vec![MyDeviceEntry {
                device_id: "device-self".to_owned(),
                label: "MacBook Pro".to_owned(),
                cert_signer_device_id: "device-self".to_owned(),
                cert_issued_at_ms: 1_700_000_000_000,
            }],
        })
        .unwrap_or_else(|error| panic!("devices.list should serialize: {error}"));

        assert_eq!(
            payload.get("type").and_then(serde_json::Value::as_str),
            Some("devices.list")
        );
        assert_eq!(
            payload
                .get("selfDeviceId")
                .and_then(serde_json::Value::as_str),
            Some("device-self")
        );
        assert_eq!(
            payload
                .get("localDeviceEnrolled")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .get("devices")
                .and_then(serde_json::Value::as_array)
                .and_then(|devices| devices.first())
                .and_then(|device| device.get("certSignerDeviceId"))
                .and_then(serde_json::Value::as_str),
            Some("device-self")
        );
    }

    #[test]
    fn serializes_device_link_events() {
        let requested = serde_json::to_value(DeviceEvent::LinkRequested {
            user_code: "ABCD-EFGH".to_owned(),
            device_label: "iPhone".to_owned(),
            expires_at: "2026-04-24T10:00:00Z".to_owned(),
        })
        .unwrap_or_else(|error| panic!("devices.link.requested should serialize: {error}"));
        assert_eq!(
            requested.get("type").and_then(serde_json::Value::as_str),
            Some("devices.link.requested")
        );

        let resolved = serde_json::to_value(DeviceEvent::LinkSelfResolved {
            outcome: SelfDeviceLinkOutcome::Approved,
        })
        .unwrap_or_else(|error| panic!("devices.link.selfResolved should serialize: {error}"));
        assert_eq!(
            resolved.get("type").and_then(serde_json::Value::as_str),
            Some("devices.link.selfResolved")
        );
        assert_eq!(
            resolved.get("outcome").and_then(serde_json::Value::as_str),
            Some("approved")
        );

        let peer_resolved = serde_json::to_value(DeviceEvent::LinkResolved {
            user_code: "ABCD-EFGH".to_owned(),
            outcome: DeviceLinkOutcome::Cancelled,
        })
        .expect("devices.link.resolved should serialize");
        assert_eq!(peer_resolved["outcome"], "cancelled");
    }

    #[test]
    fn device_link_outcome_vocabularies_are_stable() {
        for (outcome, wire) in [
            (DeviceLinkOutcome::Approved, "approved"),
            (DeviceLinkOutcome::Cancelled, "cancelled"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("link outcome"), wire);
        }
        for (outcome, wire) in [
            (SelfDeviceLinkOutcome::Approved, "approved"),
            (SelfDeviceLinkOutcome::Cancelled, "cancelled"),
            (SelfDeviceLinkOutcome::Expired, "expired"),
            (SelfDeviceLinkOutcome::Failed, "failed"),
        ] {
            assert_eq!(
                serde_json::to_value(outcome).expect("self-link outcome"),
                wire
            );
        }
    }

    #[test]
    fn deserializes_device_command_shape() {
        let payload = r#"{"type":"devices.link.approve","userCode":"BCDF-2345"}"#;
        let command: DeviceCommand = serde_json::from_str(payload)
            .unwrap_or_else(|error| panic!("device command should deserialize: {error}"));

        std::assert_matches!(
            command,
            DeviceCommand::LinkApprove { ref user_code } if user_code == "BCDF-2345"
        );
        assert_eq!(command.operation(), "link.approve");
        assert!(command.validate().is_ok());
    }

    #[test]
    fn link_approve_accepts_any_user_typed_casing_and_spacing() {
        for raw in ["bcdf2345", "BCDF-2345", " bcdf - 2345 "] {
            let command = DeviceCommand::LinkApprove {
                user_code: raw.to_owned(),
            };
            assert!(command.validate().is_ok(), "raw = {raw}");
            assert_eq!(
                command.user_code_for_error().as_deref(),
                Some("BCDF-2345"),
                "raw = {raw}"
            );
        }
    }

    #[test]
    fn link_approve_rejects_codes_that_are_not_two_groups_of_four() {
        for raw in ["ABCD-EFGH", "ABCD-EFG", "ABCD-EFGH-IJKL", "ABCD!EFG"] {
            std::assert_matches!(
                DeviceCommand::LinkApprove {
                    user_code: raw.to_owned(),
                }
                .validate(),
                Err(HostCommandValidationError::InvalidField {
                    field: "userCode",
                    ..
                }),
                "raw = {raw}"
            );
        }
    }

    #[test]
    fn only_link_approve_carries_an_error_correlation_code() {
        assert_eq!(DeviceCommand::Refresh.user_code_for_error(), None);
        assert_eq!(DeviceCommand::LinkStartSelf.user_code_for_error(), None);
        assert_eq!(
            DeviceCommand::Revoke {
                device_id: "device-1".to_owned(),
            }
            .user_code_for_error(),
            None
        );
    }

    #[test]
    fn error_omits_user_code_when_absent_and_emits_it_when_present() {
        let lane_wide = serde_json::to_value(DeviceEvent::Error {
            user_code: None,
            operation: "refresh".to_owned(),
            message: "backend unavailable".to_owned(),
        })
        .expect("devices.error should serialize");
        assert!(lane_wide.get("userCode").is_none());

        let correlated = serde_json::to_value(DeviceEvent::Error {
            user_code: Some("ABCD-EFGH".to_owned()),
            operation: "link.approve".to_owned(),
            message: "not found".to_owned(),
        })
        .expect("devices.error should serialize");
        assert_eq!(
            correlated
                .get("userCode")
                .and_then(serde_json::Value::as_str),
            Some("ABCD-EFGH")
        );
    }

    #[test]
    fn rejects_empty_device_id() {
        assert_eq!(
            DeviceCommand::Revoke {
                device_id: " ".to_owned()
            }
            .validate(),
            Err(HostCommandValidationError::EmptyField("deviceId"))
        );
    }
}
