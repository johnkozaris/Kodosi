use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};

use super::{BackendClient, Credentials, Result, SessionDto, crypto};

impl BackendClient {
    pub(super) fn close_request(credentials: &Credentials, current: &SessionDto) -> Result<Value> {
        let device = &credentials.keys.device_id;
        let preimage = crypto::terminal_close_preimage(
            &current.owner_user_id,
            current.id,
            current.incarnation_id,
            &credentials.user_id,
            device,
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &preimage)?;
        Ok(
            json!({"incarnationId":current.incarnation_id,"deviceId":device,"signature":BASE64.encode(signature)}),
        )
    }

    #[cfg(feature = "host")]
    pub(crate) async fn refuse_close(
        &self,
        credentials: &Credentials,
        dto: &SessionDto,
    ) -> Result<()> {
        let path = format!(
            "api/sessions/{}/close-request?incarnationId={}",
            dto.id, dto.incarnation_id
        );
        let _response: Value = self
            .inner
            .http
            .device(reqwest::Method::DELETE, &path, credentials, None)
            .await?;
        Ok(())
    }
}

#[cfg(feature = "host")]
pub(crate) fn close_signature_holds(dto: &SessionDto, public: &[u8]) -> bool {
    use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm as _};

    let Some(request) = &dto.close_request else {
        return false;
    };
    let Ok(preimage) = crypto::terminal_close_preimage(
        &dto.owner_user_id,
        dto.id,
        dto.incarnation_id,
        &request.user_id,
        &request.device_id,
    ) else {
        return false;
    };
    crate::identity::pins::decode(&request.signature, 3309)
        .is_ok_and(|signature| ML_DSA_65.verify_sig(public, &preimage, &signature).is_ok())
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{identity::keys::DeviceKeys, network::wire::CloseRequest};

    fn asked(keys: &DeviceKeys, user: &str) -> SessionDto {
        let mut dto: SessionDto = serde_json::from_value(json!({
            "id":Uuid::now_v7(),"incarnationId":Uuid::now_v7(),"name":"Terminal","ownerUserId":"owner",
            "ownerName":"Owner","hostDeviceId":"host","hostName":"Host","missionId":null,"missionName":null,
            "sharedWith":[],"authorizationRevision":1,"hostOnline":false,"roomSequence":0,"roomChoice":null
        }))
        .unwrap();
        let preimage = crypto::terminal_close_preimage(
            &dto.owner_user_id,
            dto.id,
            dto.incarnation_id,
            user,
            &keys.device_id,
        )
        .unwrap();
        dto.close_request = Some(CloseRequest {
            user_id: user.to_owned(),
            device_id: keys.device_id.clone(),
            signature: BASE64
                .encode(crypto::sign_control_message(keys.signing_pkcs8(), &preimage).unwrap()),
        });
        dto
    }

    #[test]
    fn a_close_request_holds_only_for_its_own_terminal_person_and_device() {
        let keys = DeviceKeys::generate().unwrap();
        let request = asked(&keys, "friend");
        assert!(close_signature_holds(&request, keys.signing_public()));
        let other = DeviceKeys::generate().unwrap();
        assert!(!close_signature_holds(&request, other.signing_public()));
        let mut elsewhere = request.clone();
        elsewhere.incarnation_id = Uuid::now_v7();
        assert!(!close_signature_holds(&elsewhere, keys.signing_public()));
        let mut someone = request.clone();
        if let Some(asked) = &mut someone.close_request {
            asked.user_id = "owner".to_owned();
        }
        assert!(!close_signature_holds(&someone, keys.signing_public()));
        let mut unasked = request;
        unasked.close_request = None;
        assert!(!close_signature_holds(&unasked, keys.signing_public()));
    }
}
