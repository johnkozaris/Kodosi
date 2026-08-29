use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedDeviceDto {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceRequest {
    pub device_id: String,
    pub kem_public_key: String,
    pub signing_public_key: String,
    pub challenge_id: String,
    pub pop_signature: String,
    pub device_certificate: String,
    pub device_certificate_signature: String,
    pub signed_device_list: String,
    pub signed_device_list_signature: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRegistrationChallengeResponse {
    pub challenge_id: String,
    pub challenge_bytes: String,
}

#[derive(Debug, Clone)]
pub struct DeviceHttpProof<'a> {
    pub user_id: &'a str,
    pub device_id: &'a str,
    pub signing_pkcs8: &'a [u8],
}
