use serde::{Deserialize, Serialize};

use super::identity::UserIdentityBundleDto;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkInitRequest {
    pub device_id: String,
    pub device_label: String,
    pub kem_public_key: String,
    pub signing_public_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkInitResponse {
    pub device_code: String,
    pub user_code: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkPendingDto {
    pub device_id: String,
    pub device_label: String,
    pub kem_public_key: String,
    pub signing_public_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkPollRequest {
    pub device_code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkAcknowledgeRequest {
    pub device_code: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkPollResponse {
    pub state: DeviceLinkPollState,
    pub device_list_generation: Option<i64>,
    pub identity_bundle: Option<UserIdentityBundleDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceLinkPollState {
    Pending,
    Approved,
    Cancelled,
    Expired,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLinkApproveRequest {
    pub user_code: String,
    pub device_certificate: String,
    pub device_certificate_signature: String,
    pub signed_device_list: String,
    pub signed_device_list_signature: String,
}

#[cfg(test)]
mod tests {
    use super::{DeviceLinkPollResponse, DeviceLinkPollState};

    #[test]
    fn poll_state_is_typed_and_tolerates_future_values() {
        let approved: DeviceLinkPollResponse =
            serde_json::from_str(r#"{"state":"approved","deviceListGeneration":3}"#)
                .expect("approved response");
        assert_eq!(approved.state, DeviceLinkPollState::Approved);

        let cancelled: DeviceLinkPollResponse =
            serde_json::from_str(r#"{"state":"cancelled"}"#).expect("cancelled response");
        assert_eq!(cancelled.state, DeviceLinkPollState::Cancelled);

        let expired: DeviceLinkPollResponse =
            serde_json::from_str(r#"{"state":"expired"}"#).expect("expired response");
        assert_eq!(expired.state, DeviceLinkPollState::Expired);

        let future: DeviceLinkPollResponse =
            serde_json::from_str(r#"{"state":"future","deviceListGeneration":null}"#)
                .expect("future response");
        assert_eq!(future.state, DeviceLinkPollState::Unknown);
    }
}
