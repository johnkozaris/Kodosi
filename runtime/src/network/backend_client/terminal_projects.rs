use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{BackendClient, Credentials, Result, SessionDto, crypto};

impl BackendClient {
    pub(super) fn project_choice(
        credentials: &Credentials,
        current: &SessionDto,
        project: Option<Uuid>,
        history: &str,
    ) -> Result<Value> {
        let sequence = (current.project_sequence + 1).max(crate::identity::now_ms());
        let device = &credentials.keys.device_id;
        let preimage = crypto::terminal_project_preimage(
            &current.owner_user_id,
            current.id,
            current.incarnation_id,
            project,
            history,
            sequence,
            device,
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &preimage)?;
        Ok(
            json!({"deviceId":device,"sequence":sequence,"signature":BASE64.encode(signature),"history":history}),
        )
    }

    #[cfg(feature = "host")]
    pub(crate) async fn project_change(
        &self,
        credentials: &Credentials,
        local: (Option<Uuid>, u64),
        dto: &SessionDto,
    ) -> Option<(Option<Uuid>, u64)> {
        let signed = dto.project_sequence > local.1 && self.signed_by_owner(credentials, dto).await;
        let change = project_change(local, dto, signed);
        if change.is_none() && dto.project_id != local.0 {
            tracing::warn!(terminal = %dto.id, "the server named a project that no device of the owner signed; the terminal stays out of it");
        }
        change
    }

    #[cfg(feature = "host")]
    async fn signed_by_owner(&self, credentials: &Credentials, dto: &SessionDto) -> bool {
        let Some(choice) = &dto.project_choice else {
            return false;
        };
        if dto.owner_user_id != credentials.user_id
            || crate::identity::recovery::is_recovery_device(&choice.device_id)
        {
            return false;
        }
        let key = |identity: crate::identity::pins::VerifiedIdentity| {
            identity
                .devices
                .get(&choice.device_id)
                .map(|certificate| certificate.sig_public_key.clone())
        };
        let mut public = self
            .known_identity(credentials, &credentials.user_id)
            .await
            .ok()
            .and_then(key);
        if public.is_none() {
            public = self
                .fetch_identity_with(credentials, &credentials.user_id)
                .await
                .ok()
                .and_then(key);
        }
        if !public.is_some_and(|public| signature_holds(dto, &public)) {
            return false;
        }
        match dto.project_id {
            Some(project) => self
                .project_holds(credentials, project, &choice.history)
                .await
                .unwrap_or(false),
            None => choice.history.is_empty(),
        }
    }
}

#[cfg(feature = "host")]
fn project_change(
    (project, sequence): (Option<Uuid>, u64),
    dto: &SessionDto,
    signed: bool,
) -> Option<(Option<Uuid>, u64)> {
    let newest = if signed {
        dto.project_sequence
    } else {
        sequence
    };
    if dto.project_id == project {
        return signed.then_some((project, newest));
    }
    (dto.project_id.is_none() || signed).then_some((dto.project_id, newest))
}

#[cfg(feature = "host")]
fn signature_holds(dto: &SessionDto, public: &[u8]) -> bool {
    use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm as _};

    let Some(choice) = &dto.project_choice else {
        return false;
    };
    let Ok(preimage) = crypto::terminal_project_preimage(
        &dto.owner_user_id,
        dto.id,
        dto.incarnation_id,
        dto.project_id,
        &choice.history,
        dto.project_sequence,
        &choice.device_id,
    ) else {
        return false;
    };
    crate::identity::pins::decode(&choice.signature, 3309)
        .is_ok_and(|signature| ML_DSA_65.verify_sig(public, &preimage, &signature).is_ok())
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use super::*;
    use crate::{identity::keys::DeviceKeys, network::wire::ProjectChoice};

    fn dto(project: Option<Uuid>, sequence: u64) -> SessionDto {
        serde_json::from_value(json!({
            "id":Uuid::now_v7(),"incarnationId":Uuid::now_v7(),"name":"Terminal","ownerUserId":"owner",
            "ownerName":"Owner","hostDeviceId":"host","hostName":"Host","projectId":project,"projectName":null,
            "sharedWith":[],"authorizationRevision":1,"hostOnline":true,"projectSequence":sequence,"projectChoice":null,
            "hidden":false
        }))
        .unwrap()
    }

    fn signed(keys: &DeviceKeys, mut dto: SessionDto) -> SessionDto {
        let history = "a".repeat(64);
        let preimage = crypto::terminal_project_preimage(
            &dto.owner_user_id,
            dto.id,
            dto.incarnation_id,
            dto.project_id,
            &history,
            dto.project_sequence,
            &keys.device_id,
        )
        .unwrap();
        dto.project_choice = Some(ProjectChoice {
            device_id: keys.device_id.clone(),
            signature: BASE64
                .encode(crypto::sign_control_message(keys.signing_pkcs8(), &preimage).unwrap()),
            history,
        });
        dto
    }

    #[test]
    fn a_host_takes_a_project_only_with_a_newer_signed_choice() {
        let project = Some(Uuid::now_v7());
        let claimed = dto(project, 7);
        assert_eq!(project_change((None, 0), &claimed, false), None);
        assert_eq!(
            project_change((None, 0), &claimed, true),
            Some((project, 7))
        );
        assert_eq!(project_change((project, 3), &claimed, false), None);
        assert_eq!(
            project_change((project, 3), &claimed, true),
            Some((project, 7))
        );
    }

    #[test]
    fn a_host_leaves_a_project_without_a_signature_and_keeps_its_newest_sequence() {
        let project = Some(Uuid::now_v7());
        let left = dto(None, 9);
        assert_eq!(project_change((project, 5), &left, false), Some((None, 5)));
        assert_eq!(project_change((project, 5), &left, true), Some((None, 9)));
        assert_eq!(project_change((None, 5), &left, false), None);
    }

    #[test]
    fn a_signature_holds_only_for_its_own_terminal_project_and_sequence() {
        let keys = DeviceKeys::generate().unwrap();
        let choice = signed(&keys, dto(Some(Uuid::now_v7()), 11));
        assert!(signature_holds(&choice, keys.signing_public()));
        let other = DeviceKeys::generate().unwrap();
        assert!(!signature_holds(&choice, other.signing_public()));
        let mut moved = choice.clone();
        moved.project_id = Some(Uuid::now_v7());
        assert!(!signature_holds(&moved, keys.signing_public()));
        let mut replayed = choice.clone();
        replayed.project_sequence = 12;
        assert!(!signature_holds(&replayed, keys.signing_public()));
        let mut forked = choice.clone();
        if let Some(signed) = forked.project_choice.as_mut() {
            signed.history = "b".repeat(64);
        }
        assert!(!signature_holds(&forked, keys.signing_public()));
        let mut elsewhere = choice;
        elsewhere.incarnation_id = Uuid::now_v7();
        assert!(!signature_holds(&elsewhere, keys.signing_public()));
    }
}
