use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use time::OffsetDateTime;

use crate::{
    AppError, Result,
    identity_core::{device_cert, device_keys::DeviceKeys, signed_device_list},
};

#[derive(Debug, Clone)]
pub(crate) struct PendingDeviceLink {
    pub(crate) device_id: String,
    pub(crate) device_label: String,
    pub(crate) kem_public_key: String,
    pub(crate) signing_public_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceLinkApprovalRequest {
    pub(crate) user_code: String,
    pub(crate) device_certificate: String,
    pub(crate) device_certificate_signature: String,
    pub(crate) signed_device_list: String,
    pub(crate) signed_device_list_signature: String,
}

pub(crate) fn build_approval_request(
    user_id: &str,
    approver_keys: &DeviceKeys,
    previous_generation: u64,
    previous_entries: &[signed_device_list::DeviceListEntry],
    pending: &PendingDeviceLink,
    user_code: &str,
) -> Result<DeviceLinkApprovalRequest> {
    let new_kem_pub =
        BASE64
            .decode(&pending.kem_public_key)
            .map_err(|_| AppError::InvalidBackendData {
                field: "device_link.kem_public_key".to_owned(),
                reason: "base64 decode failed".to_owned(),
            })?;
    let new_sig_pub =
        BASE64
            .decode(&pending.signing_public_key)
            .map_err(|_| AppError::InvalidBackendData {
                field: "device_link.signing_public_key".to_owned(),
                reason: "base64 decode failed".to_owned(),
            })?;

    let signing_key = approver_keys.signing_key()?;
    #[expect(
        clippy::cast_sign_loss,
        reason = "OffsetDateTime::unix_timestamp is non-negative within Unix epoch range"
    )]
    let now_ms = OffsetDateTime::now_utc().unix_timestamp() as u64 * 1000;

    let signed_cert = device_cert::build_cert_for(
        user_id,
        &pending.device_id,
        &pending.device_label,
        &new_kem_pub,
        &new_sig_pub,
        &approver_keys.device_id,
        &signing_key,
        now_ms,
        None,
    )?;

    let mut next_entries = previous_entries.to_vec();
    next_entries.push(signed_device_list::DeviceListEntry {
        device_id: pending.device_id.clone(),
        signer_device_id: approver_keys.device_id.clone(),
    });

    let signed_list = signed_device_list::build_replacement_list(
        user_id,
        previous_generation,
        previous_entries,
        next_entries,
        &approver_keys.device_id,
        &signing_key,
        now_ms,
        None,
    )?;

    Ok(DeviceLinkApprovalRequest {
        user_code: user_code.to_owned(),
        device_certificate: BASE64.encode(&signed_cert.body_bytes),
        device_certificate_signature: BASE64.encode(&signed_cert.signature),
        signed_device_list: BASE64.encode(&signed_list.body_bytes),
        signed_device_list_signature: BASE64.encode(&signed_list.signature),
    })
}

pub(crate) use kodosi_domain::device_link::normalize_user_code;
