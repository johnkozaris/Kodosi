use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{BackendClient, Credentials, Result, SessionDto, crypto};

impl BackendClient {
    pub(super) fn room_choice(
        credentials: &Credentials,
        current: &SessionDto,
        room: Option<Uuid>,
    ) -> Result<Value> {
        let sequence = (current.room_sequence + 1).max(crate::identity::now_ms());
        let device = &credentials.keys.device_id;
        let preimage = crypto::terminal_room_preimage(
            &current.owner_user_id,
            current.id,
            current.incarnation_id,
            room,
            sequence,
            device,
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &preimage)?;
        Ok(json!({"deviceId":device,"sequence":sequence,"signature":BASE64.encode(signature)}))
    }

    #[cfg(feature = "host")]
    pub(crate) async fn room_change(
        &self,
        credentials: &Credentials,
        local: (Option<Uuid>, u64),
        dto: &SessionDto,
    ) -> Option<(Option<Uuid>, u64)> {
        let signed = dto.room_sequence > local.1 && self.signed_by_owner(credentials, dto).await;
        let change = room_change(local, dto, signed);
        if change.is_none() && dto.mission_id != local.0 {
            tracing::warn!(terminal = %dto.id, "the server named a room that no device of the owner signed; the terminal stays out of it");
        }
        change
    }

    #[cfg(feature = "host")]
    async fn signed_by_owner(&self, credentials: &Credentials, dto: &SessionDto) -> bool {
        let Some(choice) = &dto.room_choice else {
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
        public.is_some_and(|public| signature_holds(dto, &public))
    }
}

#[cfg(feature = "host")]
fn room_change(
    (room, sequence): (Option<Uuid>, u64),
    dto: &SessionDto,
    signed: bool,
) -> Option<(Option<Uuid>, u64)> {
    let newest = if signed { dto.room_sequence } else { sequence };
    if dto.mission_id == room {
        return signed.then_some((room, newest));
    }
    (dto.mission_id.is_none() || signed).then_some((dto.mission_id, newest))
}

#[cfg(feature = "host")]
fn signature_holds(dto: &SessionDto, public: &[u8]) -> bool {
    use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm as _};

    let Some(choice) = &dto.room_choice else {
        return false;
    };
    let Ok(preimage) = crypto::terminal_room_preimage(
        &dto.owner_user_id,
        dto.id,
        dto.incarnation_id,
        dto.mission_id,
        dto.room_sequence,
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
    use crate::{identity::keys::DeviceKeys, network::wire::RoomChoice};

    fn dto(room: Option<Uuid>, sequence: u64) -> SessionDto {
        serde_json::from_value(json!({
            "id":Uuid::now_v7(),"incarnationId":Uuid::now_v7(),"name":"Terminal","ownerUserId":"owner",
            "ownerName":"Owner","hostDeviceId":"host","hostName":"Host","missionId":room,"missionName":null,
            "sharedWith":[],"authorizationRevision":1,"hostOnline":true,"roomSequence":sequence,"roomChoice":null
        }))
        .unwrap()
    }

    fn signed(keys: &DeviceKeys, mut dto: SessionDto) -> SessionDto {
        let preimage = crypto::terminal_room_preimage(
            &dto.owner_user_id,
            dto.id,
            dto.incarnation_id,
            dto.mission_id,
            dto.room_sequence,
            &keys.device_id,
        )
        .unwrap();
        dto.room_choice = Some(RoomChoice {
            device_id: keys.device_id.clone(),
            signature: BASE64
                .encode(crypto::sign_control_message(keys.signing_pkcs8(), &preimage).unwrap()),
        });
        dto
    }

    #[test]
    fn a_host_takes_a_room_only_with_a_newer_signed_choice() {
        let room = Some(Uuid::now_v7());
        let claimed = dto(room, 7);
        assert_eq!(room_change((None, 0), &claimed, false), None);
        assert_eq!(room_change((None, 0), &claimed, true), Some((room, 7)));
        assert_eq!(room_change((room, 3), &claimed, false), None);
        assert_eq!(room_change((room, 3), &claimed, true), Some((room, 7)));
    }

    #[test]
    fn a_host_leaves_a_room_without_a_signature_and_keeps_its_newest_sequence() {
        let room = Some(Uuid::now_v7());
        let left = dto(None, 9);
        assert_eq!(room_change((room, 5), &left, false), Some((None, 5)));
        assert_eq!(room_change((room, 5), &left, true), Some((None, 9)));
        assert_eq!(room_change((None, 5), &left, false), None);
    }

    #[test]
    fn a_signature_holds_only_for_its_own_terminal_room_and_sequence() {
        let keys = DeviceKeys::generate().unwrap();
        let choice = signed(&keys, dto(Some(Uuid::now_v7()), 11));
        assert!(signature_holds(&choice, keys.signing_public()));
        let other = DeviceKeys::generate().unwrap();
        assert!(!signature_holds(&choice, other.signing_public()));
        let mut moved = choice.clone();
        moved.mission_id = Some(Uuid::now_v7());
        assert!(!signature_holds(&moved, keys.signing_public()));
        let mut replayed = choice.clone();
        replayed.room_sequence = 12;
        assert!(!signature_holds(&replayed, keys.signing_public()));
        let mut elsewhere = choice;
        elsewhere.incarnation_id = Uuid::now_v7();
        assert!(!signature_holds(&elsewhere, keys.signing_public()));
    }
}
