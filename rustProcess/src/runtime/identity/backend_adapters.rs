use crate::identity_core::identity_bundle_view::{
    ExactIdentityBundleInput, ExactIdentityDeviceInput,
};
use kodosi_backend_client::{
    api::{DeviceLinkApproveRequest, DeviceLinkPendingDto, UserIdentityBundleDto},
    http_client::BackendHttpClient,
};
use zeroize::Zeroizing;

use crate::identity_core::{
    device_link::{DeviceLinkApprovalRequest, PendingDeviceLink},
    stored_auth::AccessTokenSink,
};

impl AccessTokenSink for BackendHttpClient {
    fn set_access_token(&mut self, token: Option<Zeroizing<String>>) {
        self.set_access_token(token);
    }
}

pub(crate) fn pending_device_link(dto: DeviceLinkPendingDto) -> PendingDeviceLink {
    PendingDeviceLink {
        device_id: dto.device_id,
        device_label: dto.device_label,
        kem_public_key: dto.kem_public_key,
        signing_public_key: dto.signing_public_key,
    }
}

pub(crate) fn device_link_approval_request(
    request: DeviceLinkApprovalRequest,
) -> DeviceLinkApproveRequest {
    DeviceLinkApproveRequest {
        user_code: request.user_code,
        device_certificate: request.device_certificate,
        device_certificate_signature: request.device_certificate_signature,
        signed_device_list: request.signed_device_list,
        signed_device_list_signature: request.signed_device_list_signature,
    }
}

pub(crate) fn identity_bundle_view(
    dto: &UserIdentityBundleDto,
) -> crate::Result<crate::identity_core::device_list_pin_store::IdentityBundleView> {
    if dto.identity_revision == 0 {
        return Err(crate::AppError::InvalidBackendData {
            field: "identity.identityRevision".to_owned(),
            reason: "identity revision must be positive".to_owned(),
        });
    }
    if dto.identity_incarnation_id.is_nil() {
        return Err(crate::AppError::InvalidBackendData {
            field: "identity.identityIncarnationId".to_owned(),
            reason: "identity incarnation ID must be non-nil".to_owned(),
        });
    }
    let mut view = crate::identity_core::identity_bundle_view::build_exact_bundle_view(
        &ExactIdentityBundleInput {
            user_id: dto.user_id.clone(),
            list_body: dto.device_list.body.clone(),
            list_signature: dto.device_list.signature.clone(),
            devices: dto.devices.iter().map(exact_device_input).collect(),
            historical_devices: dto
                .historical_devices
                .iter()
                .map(exact_device_input)
                .collect(),
        },
    )?;
    view.identity_revision = dto.identity_revision;
    view.identity_incarnation_id = dto.identity_incarnation_id;
    Ok(view)
}

fn exact_device_input(
    device: &kodosi_backend_client::api::UserDeviceCertificateDto,
) -> ExactIdentityDeviceInput {
    ExactIdentityDeviceInput {
        certificate: device.certificate.clone(),
        certificate_signature: device.certificate_signature.clone(),
    }
}
