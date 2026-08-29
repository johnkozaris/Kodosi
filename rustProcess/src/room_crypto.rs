use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::OpenOptions,
};

use aws_lc_rs::{digest, signature::VerificationAlgorithm};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kodosi_backend_client::crypto;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use zeroize::Zeroizing;

use crate::{
    AppError, Result,
    identity_core::{
        device_keys::DeviceKeys,
        device_list_pin_store::{IdentityBundleView, PinContext, PinVerdict},
    },
    runtime::Runtime,
};

const ROOM_CONTENT_VERSION: u8 = 2;
const ROOM_KEY_GENERATION: u32 = 1;
const ROOM_ROSTER_VERSION: u8 = 1;
const ROOM_ROSTER_PIN_FILE_VERSION: u8 = 3;
const ROOM_INVITATION_PROPOSAL_VERSION: u8 = 1;
const ROOM_INVITATION_DECISION_VERSION: u8 = 1;
const ROOM_INVITATION_LIFETIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MAX_FUTURE_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoomContentEnvelope {
    version: u8,
    room_id: String,
    object_id: String,
    content_kind: String,
    sender_user_id: String,
    sender_device_id: String,
    roster_generation: i64,
    issued_at_ms: u64,
    counter: u64,
    recipients: Vec<RoomContentRecipient>,
    ciphertext: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoomContentRecipient {
    user_id: String,
    device_id: String,
    wrapped_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoomContentAad<'a> {
    version: u8,
    room_id: &'a str,
    object_id: &'a str,
    content_kind: &'a str,
    sender_user_id: &'a str,
    sender_device_id: &'a str,
    roster_generation: i64,
    issued_at_ms: u64,
    counter: u64,
    recipients: &'a [RoomContentRecipient],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoomContentSigned<'a> {
    #[serde(flatten)]
    aad: RoomContentAad<'a>,
    ciphertext: &'a str,
}

struct RoomCryptoServices {
    backend: kodosi_backend_client::http_client::BackendHttpClient,
    pin_store: crate::identity_core::device_list_pin_store::DeviceListPinStoreHandle,
    roster_pins_path: std::path::PathBuf,
}

pub(crate) struct RoomCryptoContext {
    services: std::sync::Arc<RoomCryptoServices>,
    local_user_id: String,
    local_keys: DeviceKeys,
    verified_identities: HashMap<String, IdentityBundleView>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoomTaskPrivate {
    pub(crate) title: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RoomRosterSubmission {
    pub(crate) generation: i64,
    pub(crate) body: String,
    pub(crate) signature: String,
    pub(crate) signer_device_id: String,
    member_user_ids: BTreeSet<String>,
}

impl RoomRosterSubmission {
    pub(crate) fn from_committed_removal_target(
        room_id: &str,
        owner_user_id: &str,
        removed_user_id: &str,
        generation: i64,
        body: String,
        signature: String,
        signer_device_id: String,
    ) -> Result<Self> {
        let body_bytes = decode_base64_field(&body, "roomMutation.rosterBody")?;
        let roster: SignedRoomRoster =
            serde_json::from_slice(&body_bytes).map_err(AppError::Json)?;
        if roster.version != ROOM_ROSTER_VERSION
            || roster.room_id != room_id
            || roster.generation != generation
            || roster.owner_user_id != owner_user_id
            || roster.signer_device_id != signer_device_id
        {
            return Err(AppError::InvalidBackendData {
                field: "roomMutation.roster".to_owned(),
                reason: "retained signed roster does not match the committed removal target"
                    .to_owned(),
            });
        }
        let member_count = roster.member_user_ids.len();
        let member_user_ids = roster.member_user_ids.into_iter().collect::<BTreeSet<_>>();
        if member_user_ids.len() != member_count
            || !member_user_ids.contains(&roster.owner_user_id)
            || member_user_ids.contains(removed_user_id)
        {
            return Err(AppError::InvalidBackendData {
                field: "roomMutation.roster".to_owned(),
                reason: "retained signed roster has invalid committed membership".to_owned(),
            });
        }
        Ok(Self {
            generation,
            body,
            signature,
            signer_device_id,
            member_user_ids,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RoomInvitationProposalSubmission {
    pub(crate) invitation_id: String,
    pub(crate) invitee_user_id: String,
    pub(crate) proposal_body: String,
    pub(crate) proposal_signature: String,
    pub(crate) proposal_signer_device_id: String,
    pub(crate) proposed_roster: RoomRosterSubmission,
}

#[derive(Debug, Clone)]
pub(crate) struct RoomInvitationDecisionSubmission {
    pub(crate) body: String,
    pub(crate) signature: String,
    pub(crate) signer_device_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RoomInvitationDecision {
    Accepted,
    Declined,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomRosterPinBucket {
    #[serde(default)]
    pins: BTreeMap<String, RoomRosterPin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomRosterPinFile {
    version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, RoomRosterPinBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomRosterPin {
    owner_user_id: String,
    generation: i64,
    body: String,
    signature: String,
    member_user_ids: BTreeSet<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedRoomRoster {
    version: u8,
    room_id: String,
    generation: i64,
    owner_user_id: String,
    member_user_ids: Vec<String>,
    signer_device_id: String,
    issued_at_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedRoomInvitationProposal {
    version: u8,
    invitation_id: String,
    room_id: String,
    owner_user_id: String,
    invitee_user_id: String,
    base_roster_generation: i64,
    proposed_roster_generation: i64,
    proposed_roster_body_hash: String,
    expires_at_ms: u64,
    signer_device_id: String,
    issued_at_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedRoomInvitationDecision {
    version: u8,
    invitation_id: String,
    proposal_hash: String,
    room_id: String,
    invitee_user_id: String,
    decision: RoomInvitationDecision,
    signer_device_id: String,
    issued_at_ms: u64,
}

pub(crate) async fn create_roster(app: &Runtime, room_id: &str) -> Result<RoomRosterSubmission> {
    let owner_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    sign_roster(
        app,
        SignedRoomRoster {
            version: ROOM_ROSTER_VERSION,
            room_id: room_id.to_owned(),
            generation: 1,
            owner_user_id: owner_user_id.clone(),
            member_user_ids: vec![owner_user_id],
            signer_device_id: String::new(),
            issued_at_ms: current_epoch_ms()?,
        },
    )
    .await
}

fn ensure_expected_roster_generation(actual: i64, expected: i64) -> Result<()> {
    if actual != expected {
        return Err(AppError::HttpProblem {
            status: 409,
            code: Some("CONCURRENT_MODIFICATION".to_owned()),
            detail: format!("room roster generation changed from {expected} to {actual}"),
        });
    }
    Ok(())
}

pub(crate) async fn remove_roster_member(
    app: &Runtime,
    room_id: &str,
    user_id: &str,
    expected_roster_generation: i64,
) -> Result<RoomRosterSubmission> {
    let rooms = app.backend.fetch_rooms().await?;
    let room = rooms
        .into_iter()
        .find(|room| room.id == room_id)
        .ok_or(AppError::NotFound)?;
    ensure_expected_roster_generation(room.roster_generation, expected_roster_generation)?;
    let local_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    if local_user_id != room.owner_user_id {
        return Err(AppError::Unsupported {
            reason: "only the room owner can sign a roster change".to_owned(),
        });
    }

    let mut crypto = RoomCryptoContext::new(app).await?;
    let mut members = crypto.verify_room_roster(&room).await?;
    members.remove(user_id);
    members.insert(room.owner_user_id.clone());
    let next_generation = next_roster_generation(room.roster_generation)?;
    sign_roster(
        app,
        SignedRoomRoster {
            version: ROOM_ROSTER_VERSION,
            room_id: room.id,
            generation: next_generation,
            owner_user_id: room.owner_user_id,
            member_user_ids: members.into_iter().collect(),
            signer_device_id: String::new(),
            issued_at_ms: current_epoch_ms()?,
        },
    )
    .await
}

pub(crate) async fn create_invitation_proposal(
    app: &Runtime,
    invitation_id: &str,
    room_id: &str,
    invitee_user_id: &str,
) -> Result<RoomInvitationProposalSubmission> {
    let room = app
        .backend
        .fetch_rooms()
        .await?
        .into_iter()
        .find(|room| room.id == room_id)
        .ok_or(AppError::NotFound)?;
    let local_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    if local_user_id != room.owner_user_id {
        return Err(AppError::Unsupported {
            reason: "only the room owner can propose an invitation".to_owned(),
        });
    }

    let mut crypto = RoomCryptoContext::new(app).await?;
    let mut members = crypto.verify_room_roster(&room).await?;
    if !members.insert(invitee_user_id.to_owned()) {
        return Err(AppError::Unsupported {
            reason: "the invitee is already in the signed room roster".to_owned(),
        });
    }
    let issued_at_ms = current_epoch_ms()?;
    let next_generation = next_roster_generation(room.roster_generation)?;
    let proposed_roster = sign_roster_with_keys(
        &crypto.local_keys,
        SignedRoomRoster {
            version: ROOM_ROSTER_VERSION,
            room_id: room.id.clone(),
            generation: next_generation,
            owner_user_id: room.owner_user_id.clone(),
            member_user_ids: members.into_iter().collect(),
            signer_device_id: String::new(),
            issued_at_ms,
        },
    )?;
    let proposed_roster_body =
        decode_base64_field(&proposed_roster.body, "roomInvitation.proposedRosterBody")?;
    let expires_at_ms = issued_at_ms
        .checked_add(ROOM_INVITATION_LIFETIME_MS)
        .ok_or_else(|| AppError::Unsupported {
            reason: "invitation expiry exceeds u64 milliseconds".to_owned(),
        })?;
    let proposal = SignedRoomInvitationProposal {
        version: ROOM_INVITATION_PROPOSAL_VERSION,
        invitation_id: invitation_id.to_owned(),
        room_id: room.id,
        owner_user_id: room.owner_user_id,
        invitee_user_id: invitee_user_id.to_owned(),
        base_roster_generation: room.roster_generation,
        proposed_roster_generation: proposed_roster.generation,
        proposed_roster_body_hash: BASE64.encode(sha256(&proposed_roster_body)),
        expires_at_ms,
        signer_device_id: crypto.local_keys.device_id.clone(),
        issued_at_ms,
    };
    let proposal_body = serde_json::to_vec(&proposal).map_err(AppError::Json)?;
    let proposal_signature = crypto::sign_control_message(
        crypto.local_keys.signing_pkcs8_bytes(),
        &domain_preimage(
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
            &proposal_body,
        ),
    )?;
    Ok(RoomInvitationProposalSubmission {
        invitation_id: invitation_id.to_owned(),
        invitee_user_id: invitee_user_id.to_owned(),
        proposal_body: BASE64.encode(proposal_body),
        proposal_signature: BASE64.encode(proposal_signature),
        proposal_signer_device_id: crypto.local_keys.device_id.clone(),
        proposed_roster,
    })
}

pub(crate) fn commit_roster_pin(
    app: &Runtime,
    room_id: &str,
    owner_user_id: &str,
    roster: &RoomRosterSubmission,
) -> Result<()> {
    let account_user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    persist_roster_pin_at(
        &app.room_roster_pins_path,
        &account_user_id,
        room_id,
        RoomRosterPin {
            owner_user_id: owner_user_id.to_owned(),
            generation: roster.generation,
            body: roster.body.clone(),
            signature: roster.signature.clone(),
            member_user_ids: roster.member_user_ids.clone(),
        },
    )
}

pub(crate) async fn verified_room_members_with_services(
    backend: kodosi_backend_client::http_client::BackendHttpClient,
    pin_store: crate::identity_core::device_list_pin_store::DeviceListPinStoreHandle,
    roster_pins_path: std::path::PathBuf,
    local_user_id: String,
    local_keys: DeviceKeys,
    room_id: &str,
) -> Result<BTreeSet<String>> {
    let room = backend
        .fetch_rooms()
        .await?
        .into_iter()
        .find(|room| room.id == room_id)
        .ok_or(AppError::NotFound)?;
    let mut crypto = RoomCryptoContext::from_parts(
        backend,
        pin_store,
        roster_pins_path,
        local_user_id,
        local_keys,
    );
    crypto.verify_room_roster(&room).await
}

pub(crate) async fn verified_room_members(
    app: &Runtime,
    room_id: &str,
) -> Result<BTreeSet<String>> {
    let room = app
        .backend
        .fetch_rooms()
        .await?
        .into_iter()
        .find(|room| room.id == room_id)
        .ok_or(AppError::NotFound)?;
    let mut crypto = RoomCryptoContext::new(app).await?;
    crypto.verify_room_roster(&room).await
}

async fn sign_roster(app: &Runtime, roster: SignedRoomRoster) -> Result<RoomRosterSubmission> {
    let keys = crate::runtime::identity::device_keys::load_registered_device_keys_required(
        &app.state.identity.auth,
        &app.backend,
        &app.device_key_store,
        &app.pin_store,
    )
    .await?;
    sign_roster_with_keys(&keys, roster)
}

fn sign_roster_with_keys(
    keys: &DeviceKeys,
    mut roster: SignedRoomRoster,
) -> Result<RoomRosterSubmission> {
    roster.signer_device_id.clone_from(&keys.device_id);
    roster.member_user_ids.sort();
    roster.member_user_ids.dedup();
    let member_user_ids = roster.member_user_ids.iter().cloned().collect();
    let body = serde_json::to_vec(&roster).map_err(AppError::Json)?;
    let signature = crypto::sign_control_message(
        keys.signing_pkcs8_bytes(),
        &domain_preimage(kodosi_domain::domain_tags::ROOM_ROSTER_V1, &body),
    )?;
    Ok(RoomRosterSubmission {
        generation: roster.generation,
        body: BASE64.encode(body),
        signature: BASE64.encode(signature),
        signer_device_id: keys.device_id.clone(),
        member_user_ids,
    })
}

impl RoomCryptoContext {
    pub(crate) async fn new(app: &Runtime) -> Result<Self> {
        let local_user_id = app
            .state
            .identity
            .auth
            .subject_string()
            .ok_or(AppError::Unauthorized)?;
        let local_keys =
            crate::runtime::identity::device_keys::load_registered_device_keys_required(
                &app.state.identity.auth,
                &app.backend,
                &app.device_key_store,
                &app.pin_store,
            )
            .await?;
        Ok(Self::from_parts(
            app.backend.clone(),
            app.pin_store.clone(),
            app.room_roster_pins_path.clone(),
            local_user_id,
            local_keys,
        ))
    }

    pub(crate) fn from_parts(
        backend: kodosi_backend_client::http_client::BackendHttpClient,
        pin_store: crate::identity_core::device_list_pin_store::DeviceListPinStoreHandle,
        roster_pins_path: std::path::PathBuf,
        local_user_id: String,
        local_keys: DeviceKeys,
    ) -> Self {
        Self {
            services: std::sync::Arc::new(RoomCryptoServices {
                backend,
                pin_store,
                roster_pins_path,
            }),
            local_user_id,
            local_keys,
            verified_identities: HashMap::new(),
        }
    }

    pub(crate) async fn sign_invitation_decision(
        &mut self,
        invitation: &kodosi_backend_client::api::BackendRoomInvitation,
        decision: RoomInvitationDecision,
    ) -> Result<RoomInvitationDecisionSubmission> {
        let proposal_hash = self.verify_pending_invitation(invitation).await?;
        if self.local_user_id != invitation.invitee_user_id {
            return Err(AppError::Unsupported {
                reason: "only the invitee can sign an invitation decision".to_owned(),
            });
        }
        let issued_at_ms = current_epoch_ms()?;
        if issued_at_ms >= offset_datetime_epoch_ms(invitation.expires_at)? {
            return Err(AppError::InvalidBackendData {
                field: "roomInvitation.expiresAt".to_owned(),
                reason: "invitation proposal has expired".to_owned(),
            });
        }
        let body = serde_json::to_vec(&SignedRoomInvitationDecision {
            version: ROOM_INVITATION_DECISION_VERSION,
            invitation_id: invitation.id.clone(),
            proposal_hash,
            room_id: invitation.room_id.clone(),
            invitee_user_id: invitation.invitee_user_id.clone(),
            decision,
            signer_device_id: self.local_keys.device_id.clone(),
            issued_at_ms,
        })
        .map_err(AppError::Json)?;
        let signature = crypto::sign_control_message(
            self.local_keys.signing_pkcs8_bytes(),
            &domain_preimage(
                kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
                &body,
            ),
        )?;
        Ok(RoomInvitationDecisionSubmission {
            body: BASE64.encode(body),
            signature: BASE64.encode(signature),
            signer_device_id: self.local_keys.device_id.clone(),
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "proposal, roster, and signer checks form one fail-closed transcript verification"
    )]
    async fn verify_pending_invitation(
        &mut self,
        invitation: &kodosi_backend_client::api::BackendRoomInvitation,
    ) -> Result<String> {
        if invitation.status != "Pending"
            || invitation.decision_body.is_some()
            || invitation.decision_signature.is_some()
            || invitation.decision_signer_device_id.is_some()
        {
            return Err(AppError::InvalidBackendData {
                field: "roomInvitation.status".to_owned(),
                reason: "only a pending invitation can be decided".to_owned(),
            });
        }

        let proposal_body =
            decode_base64_field(&invitation.proposal_body, "roomInvitation.proposalBody")?;
        let proposal: SignedRoomInvitationProposal =
            serde_json::from_slice(&proposal_body).map_err(AppError::Json)?;
        let proposed_roster_body = decode_base64_field(
            &invitation.proposed_roster_body,
            "roomInvitation.proposedRosterBody",
        )?;
        let proposed_roster: SignedRoomRoster =
            serde_json::from_slice(&proposed_roster_body).map_err(AppError::Json)?;
        let now = current_epoch_ms()?;
        let response_issued_at_ms = offset_datetime_epoch_ms(invitation.proposal_issued_at)?;
        let response_expires_at_ms = offset_datetime_epoch_ms(invitation.expires_at)?;
        if proposal.version != ROOM_INVITATION_PROPOSAL_VERSION
            || proposal.invitation_id != invitation.id
            || proposal.room_id != invitation.room_id
            || proposal.owner_user_id != invitation.invited_by_user_id
            || proposal.invitee_user_id != invitation.invitee_user_id
            || proposal.base_roster_generation != invitation.base_roster_generation
            || proposal.proposed_roster_generation != invitation.proposed_roster_generation
            || proposal.base_roster_generation.checked_add(1)
                != Some(proposal.proposed_roster_generation)
            || proposal.signer_device_id != invitation.proposal_signer_device_id
            || proposal.issued_at_ms != response_issued_at_ms
            || proposal.expires_at_ms != response_expires_at_ms
            || proposal.issued_at_ms > now.saturating_add(MAX_FUTURE_CLOCK_SKEW_MS)
            || proposal.expires_at_ms <= now
            || proposal.expires_at_ms <= proposal.issued_at_ms
            || proposal.expires_at_ms.saturating_sub(proposal.issued_at_ms)
                > ROOM_INVITATION_LIFETIME_MS
        {
            return Err(AppError::InvalidBackendData {
                field: "roomInvitation.proposal".to_owned(),
                reason: "signed invitation proposal metadata does not match the invitation"
                    .to_owned(),
            });
        }
        let proposed_hash = sha256(&proposed_roster_body);
        if decode_base64_field(
            &proposal.proposed_roster_body_hash,
            "roomInvitation.proposedRosterBodyHash",
        )? != proposed_hash
        {
            return Err(AppError::InvalidBackendData {
                field: "roomInvitation.proposedRosterBody".to_owned(),
                reason: "proposed roster body hash does not match the signed proposal".to_owned(),
            });
        }

        let proposal_hash = sha256(&proposal_body);
        if decode_base64_field(&invitation.proposal_hash, "roomInvitation.proposalHash")?
            != proposal_hash
        {
            return Err(AppError::InvalidBackendData {
                field: "roomInvitation.proposalHash".to_owned(),
                reason: "proposal hash does not match the signed proposal body".to_owned(),
            });
        }

        let owner = self
            .verified_identity(&invitation.invited_by_user_id)
            .await?;
        verify_identity_signature_at(
            &owner,
            &invitation.proposal_signer_device_id,
            proposal.issued_at_ms,
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
            &proposal_body,
            &invitation.proposal_signature,
            "roomInvitation.proposalSignature",
        )?;
        verify_identity_signature_at(
            &owner,
            &invitation.proposed_roster_signer_device_id,
            proposed_roster.issued_at_ms,
            kodosi_domain::domain_tags::ROOM_ROSTER_V1,
            &proposed_roster_body,
            &invitation.proposed_roster_signature,
            "roomInvitation.proposedRosterSignature",
        )?;
        let proposed_members = proposed_roster
            .member_user_ids
            .into_iter()
            .collect::<BTreeSet<_>>();
        if proposed_roster.version != ROOM_ROSTER_VERSION
            || proposed_roster.room_id != invitation.room_id
            || proposed_roster.owner_user_id != invitation.invited_by_user_id
            || proposed_roster.generation != invitation.proposed_roster_generation
            || proposed_roster.signer_device_id != invitation.proposed_roster_signer_device_id
            || proposed_roster.issued_at_ms > now.saturating_add(MAX_FUTURE_CLOCK_SKEW_MS)
            || !proposed_members.contains(&invitation.invited_by_user_id)
            || !proposed_members.contains(&invitation.invitee_user_id)
        {
            return Err(AppError::InvalidBackendData {
                field: "roomInvitation.proposedRoster".to_owned(),
                reason: "proposed roster metadata or membership is invalid".to_owned(),
            });
        }

        Ok(BASE64.encode(proposal_hash))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "room encryption binds roster authority, current recipients, signature, and AEAD in one transaction"
    )]
    pub(crate) async fn encrypt_json<T: Serialize + Sync>(
        &mut self,
        room_id: &str,
        object_id: &str,
        content_kind: &str,
        plaintext: &T,
    ) -> Result<String> {
        let room = self
            .services
            .backend
            .fetch_rooms()
            .await?
            .into_iter()
            .find(|room| room.id == room_id)
            .ok_or(AppError::NotFound)?;
        let roster_generation = room.roster_generation;
        let member_ids = self.verify_room_roster(&room).await?;
        let backend_member_ids = self
            .services
            .backend
            .fetch_room_members(room_id)
            .await?
            .into_iter()
            .map(|member| member.user_id)
            .chain(std::iter::once(self.local_user_id.clone()))
            .collect::<BTreeSet<_>>();
        if !backend_member_ids.is_subset(&member_ids) {
            return Err(AppError::InvalidBackendData {
                field: "room.members".to_owned(),
                reason: "backend room membership exceeds the owner-signed roster".to_owned(),
            });
        }
        let member_ids = backend_member_ids;
        let content_key = Zeroizing::new(crypto::generate_session_key()?);
        let mut recipients = Vec::new();
        for user_id in member_ids {
            let view = self.verified_identity(&user_id).await?;
            for (device_id, device) in view.devices {
                let wrapped_key = crypto::wrap_session_key(
                    &device.certificate.kem_public_key,
                    &content_key,
                    room_id,
                    &device_id,
                    ROOM_KEY_GENERATION,
                )?;
                recipients.push(RoomContentRecipient {
                    user_id: view.user_id.clone(),
                    device_id,
                    wrapped_key: BASE64.encode(wrapped_key),
                });
            }
        }
        recipients.sort_by(|left, right| {
            (&left.user_id, &left.device_id).cmp(&(&right.user_id, &right.device_id))
        });
        recipients.dedup_by(|left, right| {
            left.user_id == right.user_id && left.device_id == right.device_id
        });
        if recipients.is_empty() {
            return Err(AppError::Unsupported {
                reason: "room has no verified recipient devices".to_owned(),
            });
        }

        let counter = random_counter()?;
        let issued_at_ms = current_epoch_ms()?;
        let aad = RoomContentAad {
            version: ROOM_CONTENT_VERSION,
            room_id,
            object_id,
            content_kind,
            sender_user_id: &self.local_user_id,
            sender_device_id: &self.local_keys.device_id,
            roster_generation,
            issued_at_ms,
            counter,
            recipients: &recipients,
        };
        let aad_bytes = serde_json::to_vec(&aad).map_err(AppError::Json)?;
        let plaintext = serde_json::to_vec(plaintext).map_err(AppError::Json)?;
        let ciphertext = crypto::encrypt_frame(
            &content_key,
            ROOM_KEY_GENERATION,
            counter,
            &aad_bytes,
            &plaintext,
        )?;
        let ciphertext = BASE64.encode(ciphertext);
        let signed = RoomContentSigned {
            aad,
            ciphertext: &ciphertext,
        };
        let signature_preimage = signature_preimage(&signed)?;
        let signature = crypto::sign_control_message(
            self.local_keys.signing_pkcs8_bytes(),
            &signature_preimage,
        )?;
        serde_json::to_string(&RoomContentEnvelope {
            version: ROOM_CONTENT_VERSION,
            room_id: room_id.to_owned(),
            object_id: object_id.to_owned(),
            content_kind: content_kind.to_owned(),
            sender_user_id: self.local_user_id.clone(),
            sender_device_id: self.local_keys.device_id.clone(),
            roster_generation,
            issued_at_ms,
            counter,
            recipients,
            ciphertext,
            signature: BASE64.encode(signature),
        })
        .map_err(AppError::Json)
    }

    pub(crate) async fn verify_room(
        &mut self,
        room: &kodosi_backend_client::api::BackendRoom,
    ) -> Result<()> {
        let members = self.verify_room_roster(room).await?;
        if !members.contains(&self.local_user_id) {
            return Err(AppError::InvalidBackendData {
                field: "room.roster".to_owned(),
                reason: "current user is not in the owner-signed room roster".to_owned(),
            });
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "room decryption verifies historical roster, signer lifetime, signature, recipient, and AEAD as one chain"
    )]
    pub(crate) async fn decrypt_json<T: DeserializeOwned>(
        &mut self,
        room_id: &str,
        object_id: &str,
        content_kind: &str,
        expected_sender_user_id: &str,
        encoded: &str,
    ) -> Result<T> {
        let envelope = parse_room_content_envelope(encoded)?;
        if envelope.version != ROOM_CONTENT_VERSION
            || envelope.room_id != room_id
            || envelope.object_id != object_id
            || envelope.content_kind != content_kind
            || envelope.sender_user_id != expected_sender_user_id
            || envelope.roster_generation <= 0
            || envelope.issued_at_ms > current_epoch_ms()?.saturating_add(MAX_FUTURE_CLOCK_SKEW_MS)
        {
            return Err(AppError::InvalidBackendData {
                field: "room.encryptedContent".to_owned(),
                reason: "signed room-content metadata does not match its backend record".to_owned(),
            });
        }
        let room = self
            .services
            .backend
            .fetch_rooms()
            .await?
            .into_iter()
            .find(|room| room.id == room_id)
            .ok_or(AppError::NotFound)?;
        self.verify_room_roster(&room).await?;
        let historical_members =
            verified_roster_members_at_generation(&room, envelope.roster_generation)?;
        ensure_sender_is_room_member(&historical_members, &envelope.sender_user_id)?;
        let sender_view = self.verified_identity(&envelope.sender_user_id).await?;
        let sender_key = sender_view
            .signing_device_at(&envelope.sender_device_id, envelope.issued_at_ms)
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "room.encryptedContent.senderDeviceId".to_owned(),
                reason: "sender device was not valid when the content was signed".to_owned(),
            })?;
        let aad = RoomContentAad {
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
        };
        let signed = RoomContentSigned {
            aad,
            ciphertext: &envelope.ciphertext,
        };
        let signature =
            BASE64
                .decode(&envelope.signature)
                .map_err(|error| AppError::InvalidBackendData {
                    field: "room.encryptedContent.signature".to_owned(),
                    reason: format!("invalid base64: {error}"),
                })?;
        aws_lc_rs::signature::ML_DSA_65
            .verify_sig(
                &sender_key.sig_public_key,
                &signature_preimage(&signed)?,
                &signature,
            )
            .map_err(|_| AppError::InvalidBackendData {
                field: "room.encryptedContent.signature".to_owned(),
                reason: "room-content signature verification failed".to_owned(),
            })?;

        let recipient = envelope
            .recipients
            .iter()
            .find(|recipient| {
                recipient.user_id == self.local_user_id
                    && recipient.device_id == self.local_keys.device_id
            })
            .ok_or_else(|| AppError::Unsupported {
                reason: "this device is not a recipient of the room content".to_owned(),
            })?;
        let wrapped_key = BASE64.decode(&recipient.wrapped_key).map_err(|error| {
            AppError::InvalidBackendData {
                field: "room.encryptedContent.wrappedKey".to_owned(),
                reason: format!("invalid base64: {error}"),
            }
        })?;
        let content_key = Zeroizing::new(crypto::unwrap_session_key(
            &self.local_keys.kem_decapsulation_key()?,
            &wrapped_key,
            room_id,
            &self.local_keys.device_id,
            ROOM_KEY_GENERATION,
        )?);
        let ciphertext =
            BASE64
                .decode(&envelope.ciphertext)
                .map_err(|error| AppError::InvalidBackendData {
                    field: "room.encryptedContent.ciphertext".to_owned(),
                    reason: format!("invalid base64: {error}"),
                })?;
        let aad_bytes = serde_json::to_vec(&signed.aad).map_err(AppError::Json)?;
        let plaintext = crypto::decrypt_frame(
            &content_key,
            ROOM_KEY_GENERATION,
            envelope.counter,
            &aad_bytes,
            &ciphertext,
        )?;
        parse_decrypted_room_content(&plaintext)
    }

    async fn verify_room_roster(
        &mut self,
        room: &kodosi_backend_client::api::BackendRoom,
    ) -> Result<BTreeSet<String>> {
        let existing = current_roster_pin_at(
            &self.services.roster_pins_path,
            &self.local_user_id,
            &room.id,
        )?;
        self.verify_roster_history(room, existing.as_ref()).await?;
        let body =
            BASE64
                .decode(&room.roster_body)
                .map_err(|error| AppError::InvalidBackendData {
                    field: "room.rosterBody".to_owned(),
                    reason: format!("invalid base64: {error}"),
                })?;
        let roster: SignedRoomRoster = serde_json::from_slice(&body).map_err(AppError::Json)?;
        if roster.version != ROOM_ROSTER_VERSION
            || roster.room_id != room.id
            || roster.generation != room.roster_generation
            || roster.owner_user_id != room.owner_user_id
            || roster.signer_device_id != room.roster_signer_device_id
            || roster.issued_at_ms > current_epoch_ms()?.saturating_add(5 * 60 * 1000)
        {
            return Err(AppError::InvalidBackendData {
                field: "room.roster".to_owned(),
                reason: "signed room roster metadata does not match the room".to_owned(),
            });
        }
        let owner = self.verified_identity(&room.owner_user_id).await?;
        verify_identity_signature_at(
            &owner,
            &room.roster_signer_device_id,
            roster.issued_at_ms,
            kodosi_domain::domain_tags::ROOM_ROSTER_V1,
            &body,
            &room.roster_signature,
            "room.rosterSignature",
        )?;
        let member_count = roster.member_user_ids.len();
        let members = roster.member_user_ids.into_iter().collect::<BTreeSet<_>>();
        if members.len() != member_count {
            return Err(AppError::InvalidBackendData {
                field: "room.roster".to_owned(),
                reason: "signed room roster contains duplicate member IDs".to_owned(),
            });
        }
        if !members.contains(&room.owner_user_id) {
            return Err(AppError::InvalidBackendData {
                field: "room.roster".to_owned(),
                reason: "owner is missing from the signed room roster".to_owned(),
            });
        }
        let incoming = RoomRosterPin {
            owner_user_id: room.owner_user_id.clone(),
            generation: room.roster_generation,
            body: room.roster_body.clone(),
            signature: room.roster_signature.clone(),
            member_user_ids: members.clone(),
        };
        match validate_roster_pin_transition(existing.as_ref(), &incoming)? {
            RosterPinTransition::Unchanged => {}
            RosterPinTransition::Changed { added_members } => {
                if !added_members.is_empty() {
                    self.verify_roster_activation(
                        room,
                        &body,
                        &members,
                        existing.as_ref(),
                        &added_members,
                    )
                    .await?;
                }
                self.persist_verified_roster_pin(&room.id, incoming)?;
            }
            RosterPinTransition::FirstSeen => {
                if room.roster_generation <= 1 {
                    if members.len() != 1 {
                        return Err(AppError::InvalidBackendData {
                            field: "room.roster".to_owned(),
                            reason: "generation 1 roster must contain only the owner; non-owner \
                                     members require an accepted invitation proof"
                                .to_owned(),
                        });
                    }
                } else if self.local_user_id != room.owner_user_id {
                    let added_members = BTreeSet::from([self.local_user_id.clone()]);
                    self.verify_roster_activation(room, &body, &members, None, &added_members)
                        .await?;
                }
                self.persist_verified_roster_pin(&room.id, incoming)?;
            }
        }
        Ok(members)
    }

    fn persist_verified_roster_pin(&self, room_id: &str, incoming: RoomRosterPin) -> Result<()> {
        persist_roster_pin_at(
            &self.services.roster_pins_path,
            &self.local_user_id,
            room_id,
            incoming,
        )
    }

    async fn verify_roster_history(
        &mut self,
        room: &kodosi_backend_client::api::BackendRoom,
        pinned: Option<&RoomRosterPin>,
    ) -> Result<()> {
        let plan = plan_roster_history(room, pinned)?;
        let owner = self.verified_identity(&room.owner_user_id).await?;
        for entry in &plan.entries {
            verify_identity_signature_at(
                &owner,
                &entry.signer_device_id,
                entry.issued_at_ms,
                kodosi_domain::domain_tags::ROOM_ROSTER_V1,
                &entry.body,
                &entry.signature,
                "room.rosterTransitions.rosterSignature",
            )?;
        }
        for admission in &plan.admissions {
            self.verify_admission_proof(
                room,
                admission.proof_index,
                &admission.roster_body,
                admission.generation,
                &admission.user_id,
            )
            .await?;
        }
        Ok(())
    }

    async fn verify_admission_proof(
        &mut self,
        room: &kodosi_backend_client::api::BackendRoom,
        proof_index: usize,
        roster_body: &[u8],
        generation: i64,
        invitee_user_id: &str,
    ) -> Result<()> {
        let proof = room
            .admission_proofs
            .get(proof_index)
            .ok_or_else(|| history_error("admission proof index out of range"))?;
        let proposal_body =
            decode_base64_field(&proof.proposal_body, "room.admissionProofs.proposalBody")?;
        let proposal: SignedRoomInvitationProposal =
            serde_json::from_slice(&proposal_body).map_err(AppError::Json)?;
        let decision_body =
            decode_base64_field(&proof.decision_body, "room.admissionProofs.decisionBody")?;
        let decision: SignedRoomInvitationDecision =
            serde_json::from_slice(&decision_body).map_err(AppError::Json)?;
        let proposal_hash = sha256(&proposal_body);

        if proof.invitee_user_id != invitee_user_id
            || proposal.version != ROOM_INVITATION_PROPOSAL_VERSION
            || proposal.invitation_id != proof.invitation_id
            || proposal.room_id != room.id
            || proposal.owner_user_id != room.owner_user_id
            || proposal.invitee_user_id != invitee_user_id
            || proposal.proposed_roster_generation != generation
            || proposal.base_roster_generation.checked_add(1) != Some(generation)
            || proposal.signer_device_id != proof.proposal_signer_device_id
            || proposal.expires_at_ms <= proposal.issued_at_ms
            || proposal.expires_at_ms.saturating_sub(proposal.issued_at_ms)
                > ROOM_INVITATION_LIFETIME_MS
            || decode_base64_field(
                &proposal.proposed_roster_body_hash,
                "room.admissionProofs.proposedRosterBodyHash",
            )? != sha256(roster_body)
            || decode_base64_field(&proof.proposal_hash, "room.admissionProofs.proposalHash")?
                != proposal_hash
        {
            return Err(AppError::InvalidBackendData {
                field: "room.admissionProofs.proposal".to_owned(),
                reason: "admission proposal does not authorize this roster generation".to_owned(),
            });
        }

        if decision.version != ROOM_INVITATION_DECISION_VERSION
            || decision.invitation_id != proof.invitation_id
            || decision.proposal_hash != BASE64.encode(&proposal_hash)
            || decision.room_id != room.id
            || decision.invitee_user_id != invitee_user_id
            || decision.decision != RoomInvitationDecision::Accepted
            || decision.signer_device_id != proof.decision_signer_device_id
            || decision.issued_at_ms < proposal.issued_at_ms
            || decision.issued_at_ms > proposal.expires_at_ms
        {
            return Err(AppError::InvalidBackendData {
                field: "room.admissionProofs.decision".to_owned(),
                reason: "admission decision does not accept the bound proposal".to_owned(),
            });
        }

        let owner = self.verified_identity(&room.owner_user_id).await?;
        verify_identity_signature_at(
            &owner,
            &proof.proposal_signer_device_id,
            proposal.issued_at_ms,
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
            &proposal_body,
            &proof.proposal_signature,
            "room.admissionProofs.proposalSignature",
        )?;
        let invitee = self.verified_identity(invitee_user_id).await?;
        verify_identity_signature_at(
            &invitee,
            &proof.decision_signer_device_id,
            decision.issued_at_ms,
            kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
            &decision_body,
            &proof.decision_signature,
            "room.admissionProofs.decisionSignature",
        )
    }

    async fn verify_roster_activation(
        &mut self,
        room: &kodosi_backend_client::api::BackendRoom,
        roster_body: &[u8],
        roster_members: &BTreeSet<String>,
        previous: Option<&RoomRosterPin>,
        added_members: &BTreeSet<String>,
    ) -> Result<()> {
        let proof = require_roster_activation_proof(room)?;
        let proposal_body = decode_base64_field(
            &proof.proposal_body,
            "room.rosterActivationProof.proposalBody",
        )?;
        let proposal: SignedRoomInvitationProposal =
            serde_json::from_slice(&proposal_body).map_err(AppError::Json)?;
        let decision_body = decode_base64_field(
            &proof.decision_body,
            "room.rosterActivationProof.decisionBody",
        )?;
        let decision: SignedRoomInvitationDecision =
            serde_json::from_slice(&decision_body).map_err(AppError::Json)?;
        let now = current_epoch_ms()?;
        let owner = self.verified_identity(&room.owner_user_id).await?;
        let invitee = self.verified_identity(&proof.invitee_user_id).await?;
        verify_roster_activation_proof(
            room,
            proof,
            &proposal,
            &proposal_body,
            &decision,
            &decision_body,
            roster_body,
            roster_members,
            previous,
            added_members,
            &self.local_user_id,
            now,
            &owner,
            &invitee,
        )
    }

    async fn verified_identity(&mut self, user_id: &str) -> Result<IdentityBundleView> {
        if let Some(view) = self.verified_identities.get(user_id) {
            return Ok(view.clone());
        }
        let bundle = self.services.backend.fetch_user_identity(user_id).await?;
        let view = crate::runtime::identity::backend_adapters::identity_bundle_view(&bundle)?;
        match self
            .services
            .pin_store
            .verify_or_pin(view.clone(), PinContext::ExplicitShare)
            .await?
        {
            PinVerdict::FirstShare | PinVerdict::AcceptUpdate | PinVerdict::AlreadyPinned => {}
            PinVerdict::Reject { reason } => {
                return Err(AppError::PeerIdentityChanged {
                    user_id: user_id.to_owned(),
                    detail: format!("{reason:?}"),
                });
            }
        }
        self.verified_identities
            .insert(user_id.to_owned(), view.clone());
        Ok(view)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RosterHistoryEntry {
    generation: i64,
    signer_device_id: String,
    issued_at_ms: u64,
    signature: String,
    body: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct RosterAdmission {
    user_id: String,
    generation: i64,
    roster_body: Vec<u8>,

    proof_index: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct RosterHistoryPlan {
    entries: Vec<RosterHistoryEntry>,
    admissions: Vec<RosterAdmission>,
    final_members: BTreeSet<String>,
}

fn verified_roster_members_at_generation(
    room: &kodosi_backend_client::api::BackendRoom,
    generation: i64,
) -> Result<BTreeSet<String>> {
    let transition = room
        .roster_transitions
        .iter()
        .find(|transition| transition.generation == generation)
        .ok_or_else(|| history_error("content references an unavailable roster generation"))?;
    parse_history_entry(room, transition).map(|(_, members)| members)
}

fn history_error(reason: &str) -> AppError {
    AppError::InvalidBackendData {
        field: "room.rosterTransitions".to_owned(),
        reason: reason.to_owned(),
    }
}

fn plan_roster_history(
    room: &kodosi_backend_client::api::BackendRoom,
    pinned: Option<&RoomRosterPin>,
) -> Result<RosterHistoryPlan> {
    let Some(last) = room.roster_transitions.last() else {
        return Err(history_error("room roster history is missing"));
    };
    if last.generation != room.roster_generation {
        return Err(history_error(
            "roster history does not terminate at the active generation",
        ));
    }
    if last.roster_body != room.roster_body
        || last.roster_signature != room.roster_signature
        || last.roster_signer_device_id != room.roster_signer_device_id
    {
        return Err(history_error(
            "active roster does not reproduce the accepted history tail",
        ));
    }

    let anchor = pinned.map_or(1, |pin| pin.generation);
    let first = room
        .roster_transitions
        .first()
        .map(|transition| transition.generation);
    if first != Some(1) && first != Some(anchor) {
        return Err(history_error(
            "roster history is not anchored at the genesis or pinned generation",
        ));
    }

    if let Some(pin) = pinned
        && !(first.is_some_and(|generation| generation <= pin.generation)
            && last.generation >= pin.generation)
    {
        return Err(history_error(
            "roster history does not cover the pinned generation",
        ));
    }

    let proof_indexes = index_admission_proofs(room)?;
    let mut plan = RosterHistoryPlan::default();
    let mut previous: Option<(i64, BTreeSet<String>)> = None;
    let mut pending: BTreeMap<String, (i64, Vec<u8>)> = BTreeMap::new();
    let mut matched_pin = false;

    for transition in &room.roster_transitions {
        if let Some((previous_generation, _)) = &previous
            && transition.generation != previous_generation.saturating_add(1)
        {
            return Err(history_error("roster history is not contiguous"));
        }
        let (body, members) = parse_history_entry(room, transition)?;

        if let Some(pin) = pinned
            && transition.generation == pin.generation
        {
            entry_matches_pin(room, transition, &members, pin)?;
            matched_pin = true;
        }

        let added = match &previous {
            None if transition.generation == 1 => {
                if members.len() != 1 {
                    return Err(history_error(
                        "generation 1 roster must contain only the owner",
                    ));
                }
                BTreeSet::new()
            }

            None => BTreeSet::new(),
            Some((_, previous_members)) => members
                .difference(previous_members)
                .cloned()
                .collect::<BTreeSet<_>>(),
        };

        update_pending_admissions(
            previous.as_ref().map(|(_, members)| members),
            &members,
            added,
            transition.generation,
            &body,
            &mut pending,
        );

        plan.entries.push(RosterHistoryEntry {
            generation: transition.generation,
            signer_device_id: transition.roster_signer_device_id.clone(),
            issued_at_ms: serde_json::from_slice::<SignedRoomRoster>(&body)
                .map_err(AppError::Json)?
                .issued_at_ms,
            signature: transition.roster_signature.clone(),
            body,
        });
        previous = Some((transition.generation, members));
    }

    if pinned.is_some() && !matched_pin {
        return Err(history_error(
            "roster history does not cover the pinned generation",
        ));
    }

    plan.final_members = previous.map(|(_, members)| members).unwrap_or_default();
    plan.admissions = resolve_active_admissions(&plan.final_members, pending, &proof_indexes)?;
    Ok(plan)
}

fn resolve_active_admissions(
    final_members: &BTreeSet<String>,
    pending: BTreeMap<String, (i64, Vec<u8>)>,
    proof_indexes: &BTreeMap<&str, usize>,
) -> Result<Vec<RosterAdmission>> {
    let mut admissions = Vec::new();
    for (user_id, (generation, roster_body)) in pending {
        if !final_members.contains(&user_id) {
            continue;
        }
        let proof_index = proof_indexes
            .get(user_id.as_str())
            .copied()
            .ok_or_else(|| {
                history_error(&format!(
                    "roster history adds {user_id} without an admission proof"
                ))
            })?;
        admissions.push(RosterAdmission {
            user_id,
            generation,
            roster_body,
            proof_index,
        });
    }
    Ok(admissions)
}

fn update_pending_admissions(
    previous_members: Option<&BTreeSet<String>>,
    members: &BTreeSet<String>,
    added: BTreeSet<String>,
    generation: i64,
    roster_body: &[u8],
    pending: &mut BTreeMap<String, (i64, Vec<u8>)>,
) {
    if let Some(previous_members) = previous_members {
        for user_id in previous_members.difference(members) {
            pending.remove(user_id);
        }
    }
    for user_id in added {
        pending.insert(user_id, (generation, roster_body.to_vec()));
    }
}

fn entry_matches_pin(
    room: &kodosi_backend_client::api::BackendRoom,
    transition: &kodosi_backend_client::api::BackendRoomRosterTransition,
    members: &BTreeSet<String>,
    pin: &RoomRosterPin,
) -> Result<()> {
    let owner_matches = pin.owner_user_id == room.owner_user_id;
    let body_matches = transition.roster_body == pin.body;
    let signature_matches = transition.roster_signature == pin.signature;
    let members_match = *members == pin.member_user_ids;
    if owner_matches && body_matches && signature_matches && members_match {
        return Ok(());
    }
    Err(history_error(
        "roster history contradicts the pinned generation",
    ))
}

fn parse_history_entry(
    room: &kodosi_backend_client::api::BackendRoom,
    transition: &kodosi_backend_client::api::BackendRoomRosterTransition,
) -> Result<(Vec<u8>, BTreeSet<String>)> {
    let body = decode_base64_field(&transition.roster_body, "room.rosterTransitions.rosterBody")?;
    let roster: SignedRoomRoster = serde_json::from_slice(&body).map_err(AppError::Json)?;
    if roster.version != ROOM_ROSTER_VERSION
        || roster.room_id != room.id.as_str()
        || roster.owner_user_id != room.owner_user_id
        || roster.generation != transition.generation
        || roster.signer_device_id != transition.roster_signer_device_id
    {
        return Err(history_error(
            "signed roster history metadata is inconsistent",
        ));
    }

    let member_count = roster.member_user_ids.len();
    let members = roster.member_user_ids.into_iter().collect::<BTreeSet<_>>();
    if members.len() != member_count {
        return Err(history_error(
            "roster history entry contains duplicate member IDs",
        ));
    }
    if !members.contains(&room.owner_user_id) {
        return Err(history_error(
            "owner is missing from a roster history entry",
        ));
    }
    Ok((body, members))
}

fn index_admission_proofs(
    room: &kodosi_backend_client::api::BackendRoom,
) -> Result<BTreeMap<&str, usize>> {
    let mut index = BTreeMap::new();
    for (position, proof) in room.admission_proofs.iter().enumerate() {
        if index
            .insert(proof.invitee_user_id.as_str(), position)
            .is_some()
        {
            return Err(AppError::InvalidBackendData {
                field: "room.admissionProofs".to_owned(),
                reason: "duplicate admission proof for the same invitee".to_owned(),
            });
        }
    }
    Ok(index)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are every independently authenticated component of one composite proof"
)]
fn verify_roster_activation_proof(
    room: &kodosi_backend_client::api::BackendRoom,
    proof: &kodosi_backend_client::api::BackendRoomInvitationProof,
    proposal: &SignedRoomInvitationProposal,
    proposal_body: &[u8],
    decision: &SignedRoomInvitationDecision,
    decision_body: &[u8],
    roster_body: &[u8],
    roster_members: &BTreeSet<String>,
    previous: Option<&RoomRosterPin>,
    added_members: &BTreeSet<String>,
    local_user_id: &str,
    now: u64,
    owner: &IdentityBundleView,
    invitee: &IdentityBundleView,
) -> Result<()> {
    validate_roster_activation_metadata(
        room,
        proof,
        proposal,
        proposal_body,
        decision,
        roster_body,
        roster_members,
        previous,
        added_members,
        local_user_id,
        now,
    )?;
    verify_identity_signature_at(
        owner,
        &proof.proposal_signer_device_id,
        proposal.issued_at_ms,
        kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
        proposal_body,
        &proof.proposal_signature,
        "room.rosterActivationProof.proposalSignature",
    )?;
    verify_identity_signature_at(
        invitee,
        &proof.decision_signer_device_id,
        decision.issued_at_ms,
        kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
        decision_body,
        &proof.decision_signature,
        "room.rosterActivationProof.decisionSignature",
    )
}

fn require_roster_activation_proof(
    room: &kodosi_backend_client::api::BackendRoom,
) -> Result<&kodosi_backend_client::api::BackendRoomInvitationProof> {
    room.roster_activation_proof
        .as_ref()
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "room.rosterActivationProof".to_owned(),
            reason: "a roster member addition requires an accepted invitation proof".to_owned(),
        })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are every independently authenticated component of one composite proof"
)]
fn validate_roster_activation_metadata(
    room: &kodosi_backend_client::api::BackendRoom,
    proof: &kodosi_backend_client::api::BackendRoomInvitationProof,
    proposal: &SignedRoomInvitationProposal,
    proposal_body: &[u8],
    decision: &SignedRoomInvitationDecision,
    roster_body: &[u8],
    roster_members: &BTreeSet<String>,
    previous: Option<&RoomRosterPin>,
    added_members: &BTreeSet<String>,
    local_user_id: &str,
    now: u64,
) -> Result<()> {
    let proposal_hash = sha256(proposal_body);
    let proof_expires_at_ms = offset_datetime_epoch_ms(proof.expires_at)?;
    if proposal.version != ROOM_INVITATION_PROPOSAL_VERSION
        || proposal.invitation_id != proof.invitation_id
        || proposal.room_id != room.id
        || proposal.owner_user_id != room.owner_user_id
        || proposal.invitee_user_id != proof.invitee_user_id
        || proposal.proposed_roster_generation != room.roster_generation
        || proposal.base_roster_generation.checked_add(1)
            != Some(proposal.proposed_roster_generation)
        || proposal.signer_device_id != proof.proposal_signer_device_id
        || proposal.expires_at_ms != proof_expires_at_ms
        || proposal.expires_at_ms <= proposal.issued_at_ms
        || proposal.expires_at_ms.saturating_sub(proposal.issued_at_ms)
            > ROOM_INVITATION_LIFETIME_MS
        || proposal.issued_at_ms > now.saturating_add(MAX_FUTURE_CLOCK_SKEW_MS)
        || decode_base64_field(
            &proposal.proposed_roster_body_hash,
            "room.rosterActivationProof.proposedRosterBodyHash",
        )? != sha256(roster_body)
        || decode_base64_field(
            &proof.proposal_hash,
            "room.rosterActivationProof.proposalHash",
        )? != proposal_hash
    {
        return Err(AppError::InvalidBackendData {
            field: "room.rosterActivationProof.proposal".to_owned(),
            reason: "invitation proposal does not authorize the active roster".to_owned(),
        });
    }

    if let Some(previous) = previous {
        let expected_members = previous
            .member_user_ids
            .iter()
            .cloned()
            .chain(std::iter::once(proof.invitee_user_id.clone()))
            .collect::<BTreeSet<_>>();
        if proposal.base_roster_generation != previous.generation
            || added_members != &BTreeSet::from([proof.invitee_user_id.clone()])
            || roster_members != &expected_members
        {
            return Err(AppError::InvalidBackendData {
                field: "room.rosterActivationProof".to_owned(),
                reason: "activation proof does not match the pinned roster transition".to_owned(),
            });
        }
    } else if proof.invitee_user_id != local_user_id || !roster_members.contains(local_user_id) {
        return Err(AppError::InvalidBackendData {
            field: "room.rosterActivationProof.inviteeUserId".to_owned(),
            reason: "first non-owner roster pin requires this user's acceptance proof".to_owned(),
        });
    }

    if decision.version != ROOM_INVITATION_DECISION_VERSION
        || decision.invitation_id != proof.invitation_id
        || decision.proposal_hash != BASE64.encode(proposal_hash)
        || decision.room_id != room.id
        || decision.invitee_user_id != proof.invitee_user_id
        || decision.decision != RoomInvitationDecision::Accepted
        || decision.signer_device_id != proof.decision_signer_device_id
        || decision.issued_at_ms < proposal.issued_at_ms
        || decision.issued_at_ms > proposal.expires_at_ms
        || decision.issued_at_ms > now.saturating_add(MAX_FUTURE_CLOCK_SKEW_MS)
    {
        return Err(AppError::InvalidBackendData {
            field: "room.rosterActivationProof.decision".to_owned(),
            reason: "invitee decision does not accept the active proposal".to_owned(),
        });
    }
    Ok(())
}

fn signature_preimage(value: &RoomContentSigned<'_>) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(value).map_err(AppError::Json)?;
    Ok(domain_preimage(
        kodosi_domain::domain_tags::ROOM_CONTENT_V2,
        &body,
    ))
}

fn parse_room_content_envelope(encoded: &str) -> Result<RoomContentEnvelope> {
    serde_json::from_str(encoded).map_err(|error| AppError::InvalidBackendData {
        field: "room.encryptedContent".to_owned(),
        reason: format!("invalid encrypted-content envelope: {error}"),
    })
}

fn parse_decrypted_room_content<T: DeserializeOwned>(plaintext: &[u8]) -> Result<T> {
    serde_json::from_slice(plaintext).map_err(|error| AppError::InvalidBackendData {
        field: "room.encryptedContent.plaintext".to_owned(),
        reason: format!("decrypted content is not valid JSON: {error}"),
    })
}

fn domain_preimage(domain_tag: &[u8], body: &[u8]) -> Vec<u8> {
    let mut preimage = Vec::with_capacity(domain_tag.len() + body.len());
    preimage.extend_from_slice(domain_tag);
    preimage.extend_from_slice(body);
    preimage
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    digest::digest(&digest::SHA256, bytes).as_ref().to_vec()
}

fn decode_base64_field(encoded: &str, field: &str) -> Result<Vec<u8>> {
    BASE64
        .decode(encoded)
        .map_err(|error| AppError::InvalidBackendData {
            field: field.to_owned(),
            reason: format!("invalid base64: {error}"),
        })
}

fn verify_identity_signature_at(
    identity: &IdentityBundleView,
    signer_device_id: &str,
    signed_at_ms: u64,
    domain_tag: &[u8],
    body: &[u8],
    encoded_signature: &str,
    field: &str,
) -> Result<()> {
    let signer = identity
        .signing_device_at(signer_device_id, signed_at_ms)
        .ok_or_else(|| AppError::InvalidBackendData {
            field: field.to_owned(),
            reason: "signer device was not valid when the artifact was signed".to_owned(),
        })?;
    verify_with_device_signature(signer, domain_tag, body, encoded_signature, field)
}

#[cfg(test)]
fn verify_identity_signature(
    identity: &IdentityBundleView,
    signer_device_id: &str,
    domain_tag: &[u8],
    body: &[u8],
    encoded_signature: &str,
    field: &str,
) -> Result<()> {
    let signer =
        identity
            .devices
            .get(signer_device_id)
            .ok_or_else(|| AppError::InvalidBackendData {
                field: field.to_owned(),
                reason: "signer is not an active device for the expected user".to_owned(),
            })?;
    verify_with_device_signature(signer, domain_tag, body, encoded_signature, field)
}

fn verify_with_device_signature(
    signer: &crate::identity_core::identity_bundle_view::VerifiedDevice,
    domain_tag: &[u8],
    body: &[u8],
    encoded_signature: &str,
    field: &str,
) -> Result<()> {
    let signature = decode_base64_field(encoded_signature, field)?;
    aws_lc_rs::signature::ML_DSA_65
        .verify_sig(
            &signer.sig_public_key,
            &domain_preimage(domain_tag, body),
            &signature,
        )
        .map_err(|_| AppError::InvalidBackendData {
            field: field.to_owned(),
            reason: "ML-DSA signature verification failed".to_owned(),
        })
}

fn ensure_sender_is_room_member(
    room_members: &BTreeSet<String>,
    sender_user_id: &str,
) -> Result<()> {
    if room_members.contains(sender_user_id) {
        return Ok(());
    }
    Err(AppError::InvalidBackendData {
        field: "room.encryptedContent.senderUserId".to_owned(),
        reason: "encrypted room content sender is not in the verified room roster".to_owned(),
    })
}

fn offset_datetime_epoch_ms(value: time::OffsetDateTime) -> Result<u64> {
    u64::try_from(value.unix_timestamp_nanos() / 1_000_000).map_err(|_| {
        AppError::InvalidBackendData {
            field: "timestamp".to_owned(),
            reason: "timestamp is before Unix epoch or exceeds u64 milliseconds".to_owned(),
        }
    })
}

fn next_roster_generation(current: i64) -> Result<i64> {
    current
        .checked_add(1)
        .ok_or_else(|| AppError::InvalidBackendData {
            field: "room.rosterGeneration".to_owned(),
            reason: "room roster generation overflowed i64".to_owned(),
        })
}

fn random_counter() -> Result<u64> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|error| AppError::Unsupported {
        reason: format!("platform RNG unavailable for room encryption: {error}"),
    })?;
    Ok(u64::from_be_bytes(bytes))
}

fn current_epoch_ms() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| AppError::Unsupported {
            reason: format!("system clock is before Unix epoch: {error}"),
        })
        .and_then(|duration| {
            u64::try_from(duration.as_millis()).map_err(|_| AppError::Unsupported {
                reason: "system clock milliseconds exceed u64".to_owned(),
            })
        })
}

fn current_roster_pin_at(
    path: &std::path::Path,
    account_user_id: &str,
    room_id: &str,
) -> Result<Option<RoomRosterPin>> {
    with_roster_pin_lock(path, || {
        Ok(load_roster_pin_file_locked(path)?
            .accounts
            .get(account_user_id)
            .and_then(|bucket| bucket.pins.get(room_id))
            .cloned())
    })
}

#[derive(Debug, PartialEq, Eq)]
enum RosterPinTransition {
    FirstSeen,
    Unchanged,
    Changed { added_members: BTreeSet<String> },
}

fn validate_roster_pin_transition(
    existing: Option<&RoomRosterPin>,
    incoming: &RoomRosterPin,
) -> Result<RosterPinTransition> {
    let Some(existing) = existing else {
        return Ok(RosterPinTransition::FirstSeen);
    };
    if incoming.owner_user_id != existing.owner_user_id
        || incoming.generation < existing.generation
        || incoming.generation == existing.generation
            && (incoming.body != existing.body
                || incoming.signature != existing.signature
                || incoming.member_user_ids != existing.member_user_ids)
    {
        return Err(AppError::PeerIdentityChanged {
            user_id: incoming.owner_user_id.clone(),
            detail: "room roster rolled back or changed at the pinned generation".to_owned(),
        });
    }
    if incoming.generation == existing.generation {
        return Ok(RosterPinTransition::Unchanged);
    }
    Ok(RosterPinTransition::Changed {
        added_members: incoming
            .member_user_ids
            .difference(&existing.member_user_ids)
            .cloned()
            .collect(),
    })
}

fn persist_roster_pin_at(
    path: &std::path::Path,
    account_user_id: &str,
    room_id: &str,
    pin: RoomRosterPin,
) -> Result<()> {
    with_roster_pin_lock(path, || {
        let mut file = load_roster_pin_file_locked(path)?;
        let pins = &mut file
            .accounts
            .entry(account_user_id.to_owned())
            .or_default()
            .pins;
        if let Some(existing) = pins.get(room_id)
            && (pin.owner_user_id != existing.owner_user_id
                || pin.generation < existing.generation
                || pin.generation == existing.generation
                    && (pin.body != existing.body
                        || pin.signature != existing.signature
                        || pin.member_user_ids != existing.member_user_ids))
        {
            return Err(AppError::PeerIdentityChanged {
                user_id: pin.owner_user_id,
                detail: "refused to replace a newer pinned room roster".to_owned(),
            });
        }
        pins.insert(room_id.to_owned(), pin);
        persist_roster_pin_file(path, &file)
    })
}

fn with_roster_pin_lock<T>(
    path: &std::path::Path,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if let Some(parent) = path.parent() {
        crate::support::platform::fs::ensure_dir(parent)?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("json.lock"))?;
    lock.lock()?;
    operation()
}

fn load_roster_pin_file_locked(path: &std::path::Path) -> Result<RoomRosterPinFile> {
    let (file, migrated) = read_roster_pin_file(path)?;
    if migrated {
        persist_roster_pin_file(path, &file)?;
    }
    Ok(file)
}

fn read_roster_pin_file(path: &std::path::Path) -> Result<(RoomRosterPinFile, bool)> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let version = serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(AppError::Json)?
                .get("version")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "roomRosterPins.version".to_owned(),
                    reason: "room roster pin file is missing a version".to_owned(),
                })?;
            match version {
                1 => Ok((migrate_v1_roster_pins(&bytes)?, true)),
                2 => Ok((migrate_v2_roster_pins(&bytes)?, true)),
                value if value == u64::from(ROOM_ROSTER_PIN_FILE_VERSION) => Ok((
                    serde_json::from_slice(&bytes).map_err(AppError::Json)?,
                    false,
                )),
                _ => Err(AppError::InvalidBackendData {
                    field: "roomRosterPins.version".to_owned(),
                    reason: format!("unsupported room roster pin file version {version}"),
                }),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((
            RoomRosterPinFile {
                version: ROOM_ROSTER_PIN_FILE_VERSION,
                accounts: BTreeMap::new(),
            },
            false,
        )),
        Err(error) => Err(AppError::Io(error)),
    }
}

fn persist_roster_pin_file(path: &std::path::Path, file: &RoomRosterPinFile) -> Result<()> {
    crate::support::storage::atomic_file::atomic_write_json(
        path,
        file,
        true,
        crate::support::storage::atomic_file::FileMode::UserPrivate,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyRoomRosterPinFileV1 {
    account_user_id: String,
    pins: BTreeMap<String, LegacyRoomRosterPin>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyRoomRosterPin {
    owner_user_id: String,
    generation: i64,
    body: String,
    signature: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyRoomRosterPinFileV2 {
    account_user_id: String,
    pins: BTreeMap<String, RoomRosterPin>,
}

fn one_account_roster_pin_file(
    account_user_id: String,
    pins: BTreeMap<String, RoomRosterPin>,
) -> RoomRosterPinFile {
    RoomRosterPinFile {
        version: ROOM_ROSTER_PIN_FILE_VERSION,
        accounts: BTreeMap::from([(account_user_id, RoomRosterPinBucket { pins })]),
    }
}

fn migrate_v1_roster_pins(bytes: &[u8]) -> Result<RoomRosterPinFile> {
    let legacy: LegacyRoomRosterPinFileV1 =
        serde_json::from_slice(bytes).map_err(AppError::Json)?;
    let mut pins = BTreeMap::new();
    for (room_id, pin) in legacy.pins {
        let body = decode_base64_field(&pin.body, "roomRosterPins.body")?;
        let roster: SignedRoomRoster = serde_json::from_slice(&body).map_err(AppError::Json)?;
        if roster.room_id != room_id
            || roster.owner_user_id != pin.owner_user_id
            || roster.generation != pin.generation
        {
            return Err(AppError::InvalidBackendData {
                field: "roomRosterPins".to_owned(),
                reason: "legacy room roster pin metadata is inconsistent".to_owned(),
            });
        }
        pins.insert(
            room_id,
            RoomRosterPin {
                owner_user_id: pin.owner_user_id,
                generation: pin.generation,
                body: pin.body,
                signature: pin.signature,
                member_user_ids: roster.member_user_ids.into_iter().collect(),
            },
        );
    }
    Ok(one_account_roster_pin_file(legacy.account_user_id, pins))
}

fn migrate_v2_roster_pins(bytes: &[u8]) -> Result<RoomRosterPinFile> {
    let legacy: LegacyRoomRosterPinFileV2 =
        serde_json::from_slice(bytes).map_err(AppError::Json)?;
    Ok(one_account_roster_pin_file(
        legacy.account_user_id,
        legacy.pins,
    ))
}

pub(crate) fn reset_roster_pins_for_account_at(
    path: &std::path::Path,
    account_user_id: &str,
) -> Result<()> {
    with_roster_pin_lock(path, || {
        let mut file = load_roster_pin_file_locked(path)?;
        file.accounts.remove(account_user_id);
        persist_roster_pin_file(path, &file)
    })
}

#[cfg(test)]
mod tests;
