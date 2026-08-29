use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityResetRequest {
    pub challenge_id: String,
    pub signer_device_id: String,
    pub pop_signature: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitDeviceListRequest {
    pub signed_device_list: String,
    pub signed_device_list_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserIdentityBundleDto {
    pub user_id: String,
    pub identity_revision: u64,
    pub identity_incarnation_id: uuid::Uuid,
    pub device_list: UserDeviceListDto,
    pub devices: Vec<UserDeviceCertificateDto>,
    #[serde(default)]
    pub historical_devices: Vec<UserDeviceCertificateDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDeviceListDto {
    pub body: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDeviceCertificateDto {
    pub certificate: String,
    pub certificate_signature: String,
}

#[cfg(test)]
mod tests {
    use super::UserIdentityBundleDto;

    #[test]
    fn historical_device_ignores_retired_revoked_at_field() {
        let bundle: UserIdentityBundleDto = serde_json::from_value(serde_json::json!({
            "userId": "user-1",
            "identityRevision": 1,
            "identityIncarnationId": "11111111-1111-1111-1111-111111111111",
            "deviceList": {
                "body": "device-list",
                "signature": "device-list-signature"
            },
            "devices": [],
            "historicalDevices": [{
                "certificate": "certificate",
                "certificateSignature": "certificate-signature",
                "revokedAt": "2026-08-19T12:00:00Z"
            }]
        }))
        .expect("historical revokedAt remains a tolerated additive field");

        assert_eq!(bundle.historical_devices.len(), 1);
        assert_eq!(bundle.historical_devices[0].certificate, "certificate");
    }
}
