use aws_lc_rs::signature::VerificationAlgorithm as _;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kodosi_backend_client::artifact_endorsement::{
    ArtifactEndorsement, PutArtifactEndorsement, artifact_digest, endorsement_preimage,
};

use super::{RoomCryptoContext, current_epoch_ms, decode_base64_field, domain_preimage};
use crate::{AppError, Result, identity_core::identity_bundle_view::IdentityBundleView};

impl RoomCryptoContext {
    pub(super) async fn verify_artifact(
        &self,
        identity: &IdentityBundleView,
        signer_device_id: &str,
        signed_at_ms: u64,
        preimage: &[u8],
        encoded_signature: &str,
        field: &str,
    ) -> Result<()> {
        let signature = decode_base64_field(encoded_signature, field)?;
        if let Some(signer) = identity.signing_device_at(signer_device_id, signed_at_ms) {
            return aws_lc_rs::signature::ML_DSA_65
                .verify_sig(&signer.sig_public_key, preimage, &signature)
                .map_err(|_| artifact_error(field, "artifact signature verification failed"));
        }
        let digest = artifact_digest(
            &identity.user_id,
            &identity.identity_incarnation_id,
            signer_device_id,
            preimage,
            &signature,
        )?;
        let endorsements = self
            .services
            .backend
            .fetch_artifact_endorsements(&identity.user_id, &digest)
            .await?;
        if endorsements.len() > 1 {
            return Err(artifact_error(field, "duplicate artifact endorsements"));
        }
        let valid = endorsements
            .iter()
            .any(|endorsement| verify_endorsement(identity, &digest, endorsement).is_ok());
        if !valid {
            return Err(artifact_error(
                field,
                "historical artifact needs endorsement by a current device of its original author",
            ));
        }
        Ok(())
    }

    pub(super) async fn verify_historical_signature(
        &self,
        identity: &IdentityBundleView,
        signer_device_id: &str,
        signed_at_ms: u64,
        domain: &[u8],
        body: &[u8],
        signature: &str,
        field: &str,
    ) -> Result<()> {
        self.verify_artifact(
            identity,
            signer_device_id,
            signed_at_ms,
            &domain_preimage(domain, body),
            signature,
            field,
        )
        .await
    }

    async fn endorse_digest(&self, identity: &IdentityBundleView, digest: &str) -> Result<()> {
        if identity.user_id != self.local_user_id
            || !identity.devices.contains_key(&self.local_keys.device_id)
        {
            return Err(AppError::Unauthorized);
        }
        let preimage = endorsement_preimage(
            &identity.user_id,
            &identity.identity_incarnation_id,
            digest,
            &self.local_keys.device_id,
        )?;
        let signature = BASE64.encode(kodosi_backend_client::crypto::sign_control_message(
            self.local_keys.signing_pkcs8_bytes(),
            &preimage,
        )?);
        self.services
            .backend
            .put_artifact_endorsement(&PutArtifactEndorsement {
                identity_incarnation_id: identity.identity_incarnation_id,
                artifact_digest: digest,
                endorser_device_id: &self.local_keys.device_id,
                signature: &signature,
            })
            .await?;
        Ok(())
    }

    async fn preserve_artifact(
        &self,
        identity: &IdentityBundleView,
        original_signer: &str,
        signed_at_ms: u64,
        preimage: &[u8],
        signature: &str,
    ) -> Result<()> {
        self.verify_artifact(
            identity,
            original_signer,
            signed_at_ms,
            preimage,
            signature,
            "room.history",
        )
        .await?;
        let signature = decode_base64_field(signature, "room.history.signature")?;
        let digest = artifact_digest(
            &identity.user_id,
            &identity.identity_incarnation_id,
            original_signer,
            preimage,
            &signature,
        )?;
        self.endorse_digest(identity, &digest).await
    }

    pub(crate) async fn preserve_history_before_revocation(
        &mut self,
        target_device_id: &str,
    ) -> Result<()> {
        let user_id = self.local_user_id.clone();
        let identity = self.verified_identity(&user_id).await?;
        if target_device_id == self.local_keys.device_id
            || !identity.devices.contains_key(target_device_id)
        {
            return Err(AppError::Unsupported {
                reason: "Select another currently enrolled device to revoke.".to_owned(),
            });
        }
        self.preserve_prior_endorsements(&identity, target_device_id)
            .await?;
        let rooms = self.services.backend.fetch_rooms().await?;
        for room in rooms {
            self.verify_room_roster(&room).await?;
            if room.owner_user_id == user_id {
                for entry in &room.roster_transitions {
                    if entry.roster_signer_device_id != target_device_id {
                        continue;
                    }
                    let body = decode_base64_field(&entry.roster_body, "room.rosterBody")?;
                    let roster: super::SignedRoomRoster = serde_json::from_slice(&body)?;
                    self.preserve_artifact(
                        &identity,
                        target_device_id,
                        roster.issued_at_ms,
                        &domain_preimage(kodosi_domain::domain_tags::ROOM_ROSTER_V1, &body),
                        &entry.roster_signature,
                    )
                    .await?;
                }
            }
            for proof in &room.admission_proofs {
                if room.owner_user_id == user_id
                    && proof.proposal_signer_device_id == target_device_id
                {
                    let body = decode_base64_field(&proof.proposal_body, "room.proposal")?;
                    let proposal: super::SignedRoomInvitationProposal =
                        serde_json::from_slice(&body)?;
                    self.preserve_artifact(
                        &identity,
                        target_device_id,
                        proposal.issued_at_ms,
                        &domain_preimage(
                            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
                            &body,
                        ),
                        &proof.proposal_signature,
                    )
                    .await?;
                }
                if proof.invitee_user_id == user_id
                    && proof.decision_signer_device_id == target_device_id
                {
                    let body = decode_base64_field(&proof.decision_body, "room.decision")?;
                    let decision: super::SignedRoomInvitationDecision =
                        serde_json::from_slice(&body)?;
                    self.preserve_artifact(
                        &identity,
                        target_device_id,
                        decision.issued_at_ms,
                        &domain_preimage(
                            kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
                            &body,
                        ),
                        &proof.decision_signature,
                    )
                    .await?;
                }
            }
            self.preserve_room_content(&identity, target_device_id, &room.id)
                .await?;
        }
        Ok(())
    }

    async fn preserve_room_content(
        &self,
        identity: &IdentityBundleView,
        target: &str,
        room_id: &str,
    ) -> Result<()> {
        let mut cursor = 0;
        let mut total = 0;
        loop {
            let page = self
                .services
                .backend
                .fetch_room_chat(room_id, Some(cursor), Some(100))
                .await?;
            total += page.items.len();
            if total > 50_000 {
                return Err(artifact_error(
                    "room.history",
                    "chat history exceeds the preservation limit",
                ));
            }
            for message in &page.items {
                if message.author_user_id == identity.user_id {
                    self.preserve_content_envelope(
                        identity,
                        target,
                        room_id,
                        &message.id,
                        "chat",
                        &message.body,
                    )
                    .await?;
                }
            }
            if !page.has_more {
                break;
            }
            let next = page
                .next_since
                .filter(|next| *next > cursor)
                .ok_or_else(|| {
                    artifact_error("room.history", "chat history cursor did not advance")
                })?;
            cursor = next;
        }
        let mut offset = 0;
        let mut snapshot = None;
        loop {
            let page = self
                .services
                .backend
                .fetch_room_tasks(
                    room_id,
                    None,
                    None,
                    Some(offset),
                    Some(100),
                    snapshot.as_deref(),
                )
                .await?;
            snapshot = Some(page.snapshot);
            total += page.items.len();
            if total > 50_000 {
                return Err(artifact_error(
                    "room.history",
                    "room history exceeds the preservation limit",
                ));
            }
            for task in &page.items {
                if task.created_by_user_id == identity.user_id {
                    self.preserve_content_envelope(
                        identity,
                        target,
                        room_id,
                        &task.id,
                        "task",
                        &task.title,
                    )
                    .await?;
                }
                if task.result_author_user_id.as_deref() == Some(identity.user_id.as_str())
                    && let Some(result) = &task.result
                {
                    self.preserve_content_envelope(
                        identity,
                        target,
                        room_id,
                        &task.id,
                        "taskResult",
                        result,
                    )
                    .await?;
                }
            }
            if !page.has_more {
                return Ok(());
            }
            offset = page
                .next_offset
                .filter(|next| *next > offset)
                .ok_or_else(|| {
                    artifact_error("room.history", "task history cursor did not advance")
                })?;
        }
    }

    async fn preserve_content_envelope(
        &self,
        identity: &IdentityBundleView,
        target: &str,
        room_id: &str,
        object_id: &str,
        kind: &str,
        encoded: &str,
    ) -> Result<()> {
        let envelope = super::parse_room_content_envelope(encoded)?;
        if envelope.sender_device_id != target {
            return Ok(());
        }
        if envelope.version != super::ROOM_CONTENT_VERSION
            || envelope.room_id != room_id
            || envelope.object_id != object_id
            || envelope.content_kind != kind
            || envelope.sender_user_id != identity.user_id
        {
            return Err(artifact_error(
                "room.history",
                "room artifact metadata changed",
            ));
        }
        let signed = super::RoomContentSigned {
            aad: super::RoomContentAad {
                version: envelope.version,
                room_id: &envelope.room_id,
                object_id: &envelope.object_id,
                content_kind: &envelope.content_kind,
                sender_user_id: &envelope.sender_user_id,
                sender_device_id: &envelope.sender_device_id,
                roster_generation: envelope.roster_generation,
                issued_at_ms: envelope.issued_at_ms,
                counter: envelope.counter,
                recipients: &envelope.recipients,
            },
            ciphertext: &envelope.ciphertext,
        };
        self.preserve_artifact(
            identity,
            target,
            envelope.issued_at_ms,
            &super::signature_preimage(&signed)?,
            &envelope.signature,
        )
        .await
    }

    async fn preserve_prior_endorsements(
        &self,
        identity: &IdentityBundleView,
        target: &str,
    ) -> Result<()> {
        const PAGE_SIZE: usize = 100;
        for offset in (0..=50_000).step_by(PAGE_SIZE) {
            let page = self
                .services
                .backend
                .fetch_own_artifact_endorsements(offset, PAGE_SIZE)
                .await?;
            if page.len() > PAGE_SIZE || offset + page.len() > 50_000 {
                return Err(artifact_error(
                    "room.history",
                    "artifact history exceeds the preservation limit",
                ));
            }
            for endorsement in &page {
                if endorsement.user_id != identity.user_id
                    || endorsement.identity_incarnation_id != identity.identity_incarnation_id
                {
                    return Err(artifact_error(
                        "room.history",
                        "artifact inventory names another identity",
                    ));
                }
                if endorsement.endorser_device_id != target {
                    continue;
                }
                verify_endorsement(identity, &endorsement.artifact_digest, endorsement)?;
                self.endorse_digest(identity, &endorsement.artifact_digest)
                    .await?;
            }
            if page.len() < PAGE_SIZE {
                return Ok(());
            }
        }
        Err(artifact_error(
            "room.history",
            "artifact history did not terminate",
        ))
    }
}

fn verify_endorsement(
    identity: &IdentityBundleView,
    expected_digest: &str,
    endorsement: &ArtifactEndorsement,
) -> Result<()> {
    if endorsement.user_id != identity.user_id
        || endorsement.identity_incarnation_id != identity.identity_incarnation_id
        || endorsement.artifact_digest != expected_digest
    {
        return Err(artifact_error(
            "artifactEndorsement",
            "endorsement does not bind the original author's identity and artifact",
        ));
    }
    let signer = identity
        .signing_device_at(&endorsement.endorser_device_id, current_epoch_ms()?)
        .ok_or_else(|| {
            artifact_error(
                "artifactEndorsement",
                "endorsement signer is not currently authorized",
            )
        })?;
    let preimage = endorsement_preimage(
        &identity.user_id,
        &identity.identity_incarnation_id,
        expected_digest,
        &endorsement.endorser_device_id,
    )?;
    let signature = decode_base64_field(&endorsement.signature, "artifactEndorsement.signature")?;
    aws_lc_rs::signature::ML_DSA_65
        .verify_sig(&signer.sig_public_key, &preimage, &signature)
        .map_err(|_| {
            artifact_error(
                "artifactEndorsement.signature",
                "invalid endorsement signature",
            )
        })
}

#[cfg(test)]
mod tests;

fn artifact_error(field: &str, reason: &str) -> AppError {
    AppError::InvalidBackendData {
        field: field.to_owned(),
        reason: reason.to_owned(),
    }
}
