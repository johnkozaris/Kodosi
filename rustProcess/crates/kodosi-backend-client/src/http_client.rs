use std::collections::HashSet;

use bytes::BytesMut;
use futures_util::StreamExt;
use reqwest::{RequestBuilder, Response, StatusCode, Url};
use serde::{Deserialize, de::DeserializeOwned};
use zeroize::Zeroizing;

use crate::{
    BackendClientError, BackendOrigin, Result,
    config::BackendClientConfig,
    dto::device_link::{
        DeviceLinkAcknowledgeRequest, DeviceLinkApproveRequest, DeviceLinkInitRequest,
        DeviceLinkInitResponse, DeviceLinkPendingDto, DeviceLinkPollRequest,
        DeviceLinkPollResponse,
    },
    dto::devices::{
        AuthorizedDeviceDto, DeviceHttpProof, DeviceRegistrationChallengeResponse,
        RegisterDeviceRequest,
    },
    dto::friends::{FriendRequestHandleRequest, FriendRequestInboxDto},
    dto::health::BackendCompatibilityDto,
    dto::identity::{IdentityResetRequest, SubmitDeviceListRequest, UserIdentityBundleDto},
    dto::rooms::{
        AssignRoomTaskRequest, CancelRoomInvitationRequest, CreateRoomRequest,
        CreateRoomTaskRequest, PostRoomChatRequest, ReplaceRoomRosterRequest, RoomChatMessageDto,
        RoomChatPageDto, RoomDto, RoomInvitationDecisionRequest, RoomInvitationDto,
        RoomInvitationProposalRequest, RoomMemberDto, RoomMutationReceiptDto,
        RoomMutationResponseDto, RoomTaskDto, RoomTaskPageDto, UpdateRoomTaskStatusRequest,
    },
    dto::semantic_receipts::{SemanticReceiptAckRequest, SemanticReceiptDto},
    dto::session_keys::{
        ClaimNextKeyGenerationRequest, CurrentKeyGenerationDto, KeyGenerationClaimDto,
        KeyGenerationClaimStateDto, SessionKeyFetchDto, StoreKeyBlobsRequest,
    },
    dto::sessions::{
        CreateSessionRequest, GrantAccessRequest, PaginatedSessionFeedDto,
        SessionAccessMutationReceiptDto, SessionCardDto, SessionCreationReceiptDto,
        SessionDetailDto, UpdateSessionRequest,
    },
    dto::users::{UserProfileDto, UserSummaryDto},
    endpoint::{encode_path_segment, join_endpoint, parse_url},
};

#[derive(Debug, Clone)]
pub struct BackendHttpClient {
    client: reqwest::Client,
    base_url: Option<Url>,
    backend_origin: Option<BackendOrigin>,
    access_token: Option<Zeroizing<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyGenerationClaimOutcome {
    Claimed(u32),
    GenerationChanged(u32),
}

const EMPTY_BODY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const MAX_JSON_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROBLEM_RESPONSE_BYTES: usize = 256 * 1024;
const ROOM_FEED_PAGE_SIZE: &str = "100";
const MAX_ROOM_FEED_ITEMS: usize = 10_000;
const MAX_ROOM_FEED_PAGES: usize = 100;
const MAX_ROOM_CATALOG_ITEMS: usize = 10_000;
const MAX_ROOM_CATALOG_PAGES: usize = 3334;
const ROOM_CATALOG_PAGE_SIZE: usize = 3;
const MAX_ROOM_ADMISSION_PROOFS: usize = 10_000;
const MAX_ROOM_ADMISSION_PROOF_PAGES: usize = 2500;
const ROOM_ADMISSION_PROOF_PAGE_SIZE: usize = 4;
const MAX_ROOM_ROSTER_TRANSITIONS: usize = 10_000;
const MAX_ROOM_ROSTER_TRANSITION_PAGES: usize = 1250;
const ROOM_ROSTER_TRANSITION_PAGE_SIZE: usize = 8;
const ROOM_CHAT_DEFAULT_PAGE_SIZE: i32 = 100;
const ROOM_CHAT_MAX_PAGE_SIZE: i32 = 1000;
const ROOM_CHAT_HAS_MORE_HEADER: &str = "kodosi-has-more";
const ROOM_CHAT_NEXT_SINCE_HEADER: &str = "kodosi-next-since";
const ROOM_CHAT_NEXT_BEFORE_HEADER: &str = "kodosi-next-before";
const ROOM_TASK_DEFAULT_PAGE_SIZE: usize = 100;
const ROOM_TASK_MAX_PAGE_SIZE: usize = 500;
const ROOM_TASK_HAS_MORE_HEADER: &str = "kodosi-has-more";
const ROOM_TASK_NEXT_OFFSET_HEADER: &str = "kodosi-next-offset";
const ROOM_TASK_SNAPSHOT_HEADER: &str = "kodosi-task-snapshot";

impl BackendHttpClient {
    pub fn new(config: &BackendClientConfig) -> Result<Self> {
        let base_url = config.api.as_deref().map(parse_url).transpose()?;
        let backend_origin = base_url
            .as_ref()
            .map(BackendOrigin::from_base_url)
            .transpose()?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(BackendClientError::Http)?;

        Ok(Self {
            client,
            base_url,
            backend_origin,
            access_token: None,
        })
    }

    pub fn is_configured(&self) -> bool {
        self.base_url.is_some()
    }

    pub fn backend_origin(&self) -> Option<&BackendOrigin> {
        self.backend_origin.as_ref()
    }

    pub fn set_access_token(&mut self, token: Option<Zeroizing<String>>) {
        self.access_token = token;
    }

    pub fn into_access_token(self) -> Option<Zeroizing<String>> {
        self.access_token
    }

    fn authenticated(&self, request: RequestBuilder) -> RequestBuilder {
        if let Some(token) = &self.access_token {
            request.bearer_auth(token.as_str())
        } else {
            request
        }
    }

    fn api_relative_request_target(&self, url: &Url) -> Result<String> {
        let base = self
            .base_url
            .as_ref()
            .ok_or(BackendClientError::MissingConfig { key: "backend.api" })?;
        if url.origin() != base.origin() {
            return Err(BackendClientError::Protocol {
                reason: "device proof target escaped configured backend origin".to_owned(),
            });
        }
        let base_directory = base
            .join(".")
            .map_err(|error| BackendClientError::Protocol {
                reason: format!("configured backend base path is invalid: {error}"),
            })?;
        let base_prefix = base_directory.path().trim_end_matches('/');
        let route_path = url
            .path()
            .strip_prefix(base_prefix)
            .filter(|path| path.is_empty() || path.starts_with('/'))
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "device proof target escaped configured backend base path".to_owned(),
            })?;
        let route_path = if route_path.is_empty() {
            "/".to_owned()
        } else {
            route_path.to_owned()
        };
        Ok(url.query().map_or_else(
            || route_path.clone(),
            |query| format!("{route_path}?{query}"),
        ))
    }

    async fn fetch_device_http_proof_challenge(
        &self,
    ) -> Result<DeviceRegistrationChallengeResponse> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/device-proofs/challenge",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.post(url)))
            .await
    }

    async fn authenticated_device_request(
        &self,
        method: reqwest::Method,
        url: Url,
        proof: DeviceHttpProof<'_>,
        body_sha256: &str,
    ) -> Result<RequestBuilder> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        let challenge = self.fetch_device_http_proof_challenge().await?;
        let challenge_id = uuid::Uuid::parse_str(&challenge.challenge_id).map_err(|_| {
            BackendClientError::InvalidBackendData {
                field: "deviceChallenge.challengeId".to_owned(),
                reason: "challenge ID is not a UUID".to_owned(),
            }
        })?;
        let challenge_bytes = BASE64.decode(&challenge.challenge_bytes).map_err(|_| {
            BackendClientError::InvalidBackendData {
                field: "deviceChallenge.challengeBytes".to_owned(),
                reason: "challenge bytes are not base64".to_owned(),
            }
        })?;
        let path_and_query = self.api_relative_request_target(&url)?;
        let signature = crate::crypto::sign_device_http_request_proof(
            proof.signing_pkcs8,
            proof.user_id,
            proof.device_id,
            &challenge_id,
            method.as_str(),
            &path_and_query,
            body_sha256,
            &challenge_bytes,
        )?;
        Ok(self
            .authenticated(self.client.request(method, url))
            .header("X-Kodosi-Device-Id", proof.device_id)
            .header("X-Kodosi-Device-Challenge-Id", challenge_id.to_string())
            .header("X-Kodosi-Device-Signature", BASE64.encode(signature))
            .header("X-Kodosi-Body-Sha256", body_sha256))
    }

    pub async fn fetch_compatibility(&self) -> Result<BackendCompatibilityDto> {
        let url = join_endpoint(self.base_url.as_ref(), "health/ready", "backend.api")?;
        self.send_json(self.client.get(url)).await
    }

    async fn send_json<T>(&self, request: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = request.send().await.map_err(BackendClientError::Http)?;
        let response = check_status(response).await?;
        Self::decode_json_response(response).await
    }

    async fn send_empty(&self, request: RequestBuilder) -> Result<()> {
        let response = request.send().await.map_err(BackendClientError::Http)?;
        check_status(response).await?;
        Ok(())
    }

    pub async fn fetch_current_user(&self) -> Result<UserProfileDto> {
        let url = join_endpoint(self.base_url.as_ref(), "api/me", "backend.api")?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_friends(&self) -> Result<Vec<UserSummaryDto>> {
        let url = join_endpoint(self.base_url.as_ref(), "api/friends/", "backend.api")?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_friend_requests(&self) -> Result<FriendRequestInboxDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/friends/requests",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn send_friend_request(&self, username: &str) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/friends/requests",
            "backend.api",
        )?;
        let body = FriendRequestHandleRequest { username };
        self.send_empty(self.authenticated(self.client.post(url)).json(&body))
            .await
    }

    pub async fn accept_friend_request(&self, username: &str) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/friends/requests/accept",
            "backend.api",
        )?;
        let body = FriendRequestHandleRequest { username };
        self.send_empty(self.authenticated(self.client.post(url)).json(&body))
            .await
    }

    pub async fn reject_friend_request(&self, username: &str) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/friends/requests/reject",
            "backend.api",
        )?;
        let body = FriendRequestHandleRequest { username };
        self.send_empty(self.authenticated(self.client.post(url)).json(&body))
            .await
    }

    pub async fn cancel_outgoing_friend_request(&self, handle: &str) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/friends/requests/outgoing/{}",
                encode_path_segment(handle)
            ),
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.delete(url)))
            .await
    }

    pub async fn unfriend(&self, handle: &str) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/friends/{}", encode_path_segment(handle)),
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.delete(url)))
            .await
    }

    pub async fn fetch_rooms(&self) -> Result<Vec<RoomDto>> {
        let mut rooms = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_ROOM_CATALOG_PAGES {
            let mut url = join_endpoint(self.base_url.as_ref(), "api/rooms/", "backend.api")?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("limit", &ROOM_CATALOG_PAGE_SIZE.to_string());
                if let Some(cursor) = cursor.as_deref() {
                    query.append_pair("cursor", cursor);
                }
            }
            let response = self
                .authenticated(self.client.get(url))
                .send()
                .await
                .map_err(BackendClientError::Http)?;
            let response = check_status(response).await?;
            let has_more = parse_boolean_header(response.headers(), "kodosi-has-more")?;
            let next_cursor = response
                .headers()
                .get("kodosi-next-cursor")
                .map(|value| {
                    value
                        .to_str()
                        .ok()
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| BackendClientError::Protocol {
                            reason: "kodosi-next-cursor must be non-empty UTF-8".to_owned(),
                        })
                })
                .transpose()?;
            let page: Vec<RoomDto> = Self::decode_json_response(response).await?;
            if page.len() > ROOM_CATALOG_PAGE_SIZE
                || rooms.len() > MAX_ROOM_CATALOG_ITEMS.saturating_sub(page.len())
            {
                return Err(BackendClientError::Capacity {
                    resource: "room catalog",
                    reason: format!("exceeds {MAX_ROOM_CATALOG_ITEMS} rooms"),
                });
            }
            rooms.extend(page);
            if !has_more {
                if next_cursor.is_some() {
                    return Err(BackendClientError::Protocol {
                        reason: "terminal room catalog page carried a continuation".to_owned(),
                    });
                }
                for room in &mut rooms {
                    room.admission_proofs = self.fetch_room_admission_proofs(&room.id).await?;
                    room.roster_transitions = self
                        .fetch_room_roster_transitions(&room.id, room.roster_generation)
                        .await?;
                }
                return Ok(rooms);
            }
            let next = next_cursor.ok_or_else(|| BackendClientError::Protocol {
                reason: "nonterminal room catalog page omitted next cursor".to_owned(),
            })?;
            if cursor.as_deref() == Some(next.as_str()) {
                return Err(BackendClientError::Protocol {
                    reason: "room catalog continuation did not advance".to_owned(),
                });
            }
            cursor = Some(next);
        }
        Err(BackendClientError::Capacity {
            resource: "room catalog",
            reason: format!("exceeds {MAX_ROOM_CATALOG_PAGES} pages"),
        })
    }

    async fn fetch_room_admission_proofs(
        &self,
        room_id: &str,
    ) -> Result<Vec<crate::dto::rooms::RoomInvitationProofDto>> {
        let mut proofs = Vec::new();
        let mut after_user_id: Option<String> = None;
        for _ in 0..MAX_ROOM_ADMISSION_PROOF_PAGES {
            let mut url = join_endpoint(
                self.base_url.as_ref(),
                &format!(
                    "api/rooms/{}/admission-proofs",
                    encode_path_segment(room_id)
                ),
                "backend.api",
            )?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("limit", &ROOM_ADMISSION_PROOF_PAGE_SIZE.to_string());
                if let Some(cursor) = after_user_id.as_deref() {
                    query.append_pair("afterUserId", cursor);
                }
            }
            let response = self
                .authenticated(self.client.get(url))
                .send()
                .await
                .map_err(BackendClientError::Http)?;
            let response = check_status(response).await?;
            let has_more = parse_boolean_header(response.headers(), "kodosi-has-more")?;
            let next_user_id = response
                .headers()
                .get("kodosi-next-user-id")
                .map(|value| {
                    value
                        .to_str()
                        .ok()
                        .and_then(|value| uuid::Uuid::parse_str(value).ok())
                        .map(|value| value.hyphenated().to_string())
                        .ok_or_else(|| BackendClientError::Protocol {
                            reason: "kodosi-next-user-id must be a UUID".to_owned(),
                        })
                })
                .transpose()?;
            let page: Vec<crate::dto::rooms::RoomInvitationProofDto> =
                Self::decode_json_response(response).await?;
            if page.len() > ROOM_ADMISSION_PROOF_PAGE_SIZE
                || proofs.len() > MAX_ROOM_ADMISSION_PROOFS.saturating_sub(page.len())
            {
                return Err(BackendClientError::Capacity {
                    resource: "room admission proofs",
                    reason: format!("exceeds {MAX_ROOM_ADMISSION_PROOFS} records"),
                });
            }
            let previous_len = proofs.len();
            let boundary_user_id = page.last().map(|proof| proof.invitee_user_id.clone());
            proofs.extend(page);
            if !has_more {
                if next_user_id.is_some() {
                    return Err(BackendClientError::Protocol {
                        reason: "terminal admission-proof page carried a continuation".to_owned(),
                    });
                }
                return Ok(proofs);
            }
            let next = next_user_id.ok_or_else(|| BackendClientError::Protocol {
                reason: "nonterminal admission-proof page omitted next user ID".to_owned(),
            })?;
            if proofs.len() == previous_len
                || boundary_user_id.as_deref() != Some(next.as_str())
                || after_user_id.as_deref() == Some(next.as_str())
            {
                return Err(BackendClientError::Protocol {
                    reason: "admission-proof continuation did not advance".to_owned(),
                });
            }
            after_user_id = Some(next);
        }
        Err(BackendClientError::Capacity {
            resource: "room admission proofs",
            reason: format!("exceeds {MAX_ROOM_ADMISSION_PROOF_PAGES} pages"),
        })
    }

    async fn fetch_room_roster_transitions(
        &self,
        room_id: &str,
        expected_generation: i64,
    ) -> Result<Vec<crate::dto::rooms::RoomRosterTransitionDto>> {
        let mut transitions = Vec::new();
        let mut after_generation = 0_i64;
        for _ in 0..MAX_ROOM_ROSTER_TRANSITION_PAGES {
            let mut url = join_endpoint(
                self.base_url.as_ref(),
                &format!(
                    "api/rooms/{}/roster-transitions",
                    encode_path_segment(room_id)
                ),
                "backend.api",
            )?;
            url.query_pairs_mut()
                .append_pair("afterGeneration", &after_generation.to_string())
                .append_pair("limit", &ROOM_ROSTER_TRANSITION_PAGE_SIZE.to_string());
            let response = self
                .authenticated(self.client.get(url))
                .send()
                .await
                .map_err(BackendClientError::Http)?;
            let response = check_status(response).await?;
            let has_more = parse_boolean_header(response.headers(), "kodosi-has-more")?;
            let next_generation =
                parse_optional_i64_header(response.headers(), "kodosi-next-generation")?;
            let page: Vec<crate::dto::rooms::RoomRosterTransitionDto> =
                Self::decode_json_response(response).await?;
            if page.len() > ROOM_ROSTER_TRANSITION_PAGE_SIZE
                || transitions.len() > MAX_ROOM_ROSTER_TRANSITIONS.saturating_sub(page.len())
            {
                return Err(BackendClientError::Capacity {
                    resource: "room roster transitions",
                    reason: format!("exceeds {MAX_ROOM_ROSTER_TRANSITIONS} records"),
                });
            }
            for transition in page {
                if transition.generation != after_generation + 1 {
                    return Err(BackendClientError::Protocol {
                        reason: "room roster transition generations were not contiguous".to_owned(),
                    });
                }
                after_generation = transition.generation;
                transitions.push(transition);
            }
            if !has_more {
                if next_generation.is_some() || after_generation != expected_generation {
                    return Err(BackendClientError::Protocol {
                        reason: "room roster transition lineage did not terminate at the current generation"
                            .to_owned(),
                    });
                }
                return Ok(transitions);
            }
            if next_generation != Some(after_generation) {
                return Err(BackendClientError::Protocol {
                    reason: "room roster transition continuation did not match the page boundary"
                        .to_owned(),
                });
            }
        }
        Err(BackendClientError::Capacity {
            resource: "room roster transitions",
            reason: format!("exceeds {MAX_ROOM_ROSTER_TRANSITION_PAGES} pages"),
        })
    }

    pub async fn create_room(
        &self,
        room_id: &str,
        name: &str,
        slug: &str,
        roster_generation: i64,
        roster_body: &str,
        roster_signature: &str,
        roster_signer_device_id: &str,
    ) -> Result<RoomDto> {
        let url = join_endpoint(self.base_url.as_ref(), "api/rooms/", "backend.api")?;
        let body = CreateRoomRequest {
            room_id,
            name,
            slug,
            roster_generation,
            roster_body,
            roster_signature,
            roster_signer_device_id,
        };
        self.send_json(self.authenticated(self.client.post(url)).json(&body))
            .await
    }

    pub async fn fetch_room_members(&self, room_id: &str) -> Result<Vec<RoomMemberDto>> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/rooms/{}/members", encode_path_segment(room_id)),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn remove_room_member(
        &self,
        room_id: &str,
        user_id: &str,
        mutation_id: &uuid::Uuid,
        roster_generation: i64,
        roster_body: &str,
        roster_signature: &str,
        roster_signer_device_id: &str,
    ) -> Result<RoomMutationResponseDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/{}/members/{}",
                encode_path_segment(room_id),
                encode_path_segment(user_id)
            ),
            "backend.api",
        )?;
        let body = ReplaceRoomRosterRequest {
            request_id: mutation_id,
            generation: roster_generation,
            body: roster_body,
            signature: roster_signature,
            signer_device_id: roster_signer_device_id,
        };
        let response = self
            .send_json(self.authenticated(self.client.delete(url)).json(&body))
            .await?;
        validate_room_mutation_response(
            response,
            ExpectedRoomMutationResponse {
                request_id: mutation_id,
                operation: "removeMember",
                room_id,
                entity_id: user_id,
                result: "Removed",
                revision: Some(roster_generation),
                assignee: ExpectedRoomMutationAssignee::Exact(None),
            },
        )
    }

    pub async fn invite_room_member(
        &self,
        room_id: &str,
        proof: &RoomInvitationProposalRequest<'_>,
    ) -> Result<RoomInvitationDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/rooms/{}/invitations", encode_path_segment(room_id)),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.post(url)).json(proof))
            .await
    }

    pub async fn fetch_incoming_room_invitations(&self) -> Result<Vec<RoomInvitationDto>> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/rooms/invitations/incoming",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_outgoing_room_invitations(&self) -> Result<Vec<RoomInvitationDto>> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/rooms/invitations/outgoing",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn accept_room_invitation(
        &self,
        invitation_id: &str,
        room_id: &str,
        expected_revision: i64,
        decision: &RoomInvitationDecisionRequest<'_>,
    ) -> Result<RoomMutationResponseDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/invitations/{}/accept",
                encode_path_segment(invitation_id)
            ),
            "backend.api",
        )?;
        let response = self
            .send_json(self.authenticated(self.client.post(url)).json(decision))
            .await?;
        validate_room_mutation_response(
            response,
            ExpectedRoomMutationResponse {
                request_id: decision.request_id,
                operation: "acceptInvitation",
                room_id,
                entity_id: invitation_id,
                result: "Accepted",
                revision: Some(expected_revision),
                assignee: ExpectedRoomMutationAssignee::Exact(None),
            },
        )
    }

    pub async fn decline_room_invitation(
        &self,
        invitation_id: &str,
        room_id: &str,
        expected_revision: i64,
        decision: &RoomInvitationDecisionRequest<'_>,
    ) -> Result<RoomMutationResponseDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/invitations/{}/decline",
                encode_path_segment(invitation_id)
            ),
            "backend.api",
        )?;
        let response = self
            .send_json(self.authenticated(self.client.post(url)).json(decision))
            .await?;
        validate_room_mutation_response(
            response,
            ExpectedRoomMutationResponse {
                request_id: decision.request_id,
                operation: "declineInvitation",
                room_id,
                entity_id: invitation_id,
                result: "Declined",
                revision: Some(expected_revision),
                assignee: ExpectedRoomMutationAssignee::Exact(None),
            },
        )
    }

    pub async fn cancel_room_invitation(
        &self,
        invitation_id: &str,
        room_id: &str,
        expected_revision: i64,
        mutation_id: &uuid::Uuid,
    ) -> Result<RoomMutationResponseDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/invitations/{}",
                encode_path_segment(invitation_id)
            ),
            "backend.api",
        )?;
        let request = CancelRoomInvitationRequest {
            request_id: mutation_id,
        };
        let response = self
            .send_json(self.authenticated(self.client.delete(url)).json(&request))
            .await?;
        validate_room_mutation_response(
            response,
            ExpectedRoomMutationResponse {
                request_id: mutation_id,
                operation: "cancelInvitation",
                room_id,
                entity_id: invitation_id,
                result: "Cancelled",
                revision: Some(expected_revision),
                assignee: ExpectedRoomMutationAssignee::Exact(None),
            },
        )
    }

    pub async fn fetch_room_chat_tail(
        &self,
        room_id: &str,
        before: Option<i64>,
        limit: Option<i32>,
    ) -> Result<RoomChatPageDto> {
        self.fetch_room_chat_page(room_id, None, Some(before.unwrap_or(i64::MAX)), limit)
            .await
    }

    pub async fn fetch_room_chat(
        &self,
        room_id: &str,
        since: Option<i64>,
        limit: Option<i32>,
    ) -> Result<RoomChatPageDto> {
        self.fetch_room_chat_page(room_id, since, None, limit).await
    }

    async fn fetch_room_chat_page(
        &self,
        room_id: &str,
        since: Option<i64>,
        before: Option<i64>,
        limit: Option<i32>,
    ) -> Result<RoomChatPageDto> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/rooms/{}/chat", encode_path_segment(room_id)),
            "backend.api",
        )?;
        {
            let mut qp = url.query_pairs_mut();
            if let Some(s) = since {
                qp.append_pair("since", &s.to_string());
            }
            if let Some(b) = before {
                qp.append_pair("before", &b.to_string());
            }
            if let Some(l) = limit {
                qp.append_pair("limit", &l.to_string());
            }
        }
        let response = self
            .authenticated(self.client.get(url))
            .send()
            .await
            .map_err(BackendClientError::Http)?;
        let response = check_status(response).await?;
        let headers = parse_room_chat_headers(response.headers())?;
        let items: Vec<RoomChatMessageDto> = Self::decode_json_response(response).await?;
        resolve_room_chat_page(
            items,
            since,
            before,
            normalize_room_chat_limit(limit),
            headers,
        )
    }

    pub async fn fetch_room_chat_message(
        &self,
        room_id: &str,
        message_id: &str,
    ) -> Result<RoomChatMessageDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/{}/chat/{}",
                encode_path_segment(room_id),
                encode_path_segment(message_id)
            ),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn post_room_chat(
        &self,
        room_id: &str,
        message_id: &str,
        body: &str,
        author_session_id: Option<&str>,
        author_kind: &str,
        recipient_session_ids: &[String],
        recipient_user_ids: &[String],
    ) -> Result<RoomChatMessageDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/rooms/{}/chat", encode_path_segment(room_id)),
            "backend.api",
        )?;
        let req = PostRoomChatRequest {
            message_id,
            body,
            author_session_id,
            author_kind,
            recipient_session_ids,
            recipient_user_ids,
        };
        self.send_json(self.authenticated(self.client.post(url)).json(&req))
            .await
    }

    pub async fn fetch_room_task(&self, room_id: &str, task_id: &str) -> Result<RoomTaskDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/{}/tasks/{}",
                encode_path_segment(room_id),
                encode_path_segment(task_id)
            ),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_room_tasks(
        &self,
        room_id: &str,
        status: Option<&str>,
        assignee: Option<&str>,
        offset: Option<usize>,
        limit: Option<usize>,
        snapshot: Option<&str>,
    ) -> Result<RoomTaskPageDto> {
        let normalized_offset = offset.unwrap_or(0);
        let normalized_limit = limit
            .unwrap_or(ROOM_TASK_DEFAULT_PAGE_SIZE)
            .clamp(1, ROOM_TASK_MAX_PAGE_SIZE);
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/rooms/{}/tasks", encode_path_segment(room_id)),
            "backend.api",
        )?;
        {
            let mut qp = url.query_pairs_mut();
            if let Some(s) = status {
                qp.append_pair("status", s);
            }
            if let Some(a) = assignee {
                qp.append_pair("assignee", a);
            }
            qp.append_pair("offset", &normalized_offset.to_string());
            qp.append_pair("limit", &normalized_limit.to_string());
            if let Some(snapshot) = snapshot {
                qp.append_pair("snapshot", snapshot);
            }
        }
        let response = self
            .authenticated(self.client.get(url))
            .send()
            .await
            .map_err(BackendClientError::Http)?;
        let response = check_status(response).await?;
        let page_snapshot = response
            .headers()
            .get(ROOM_TASK_SNAPSHOT_HEADER)
            .and_then(|value| value.to_str().ok())
            .filter(|value| {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            .ok_or_else(|| BackendClientError::Protocol {
                reason: format!(
                    "{ROOM_TASK_SNAPSHOT_HEADER} must contain a 64-character lowercase hex snapshot"
                ),
            })?
            .to_owned();
        if snapshot.is_some_and(|requested| requested != page_snapshot) {
            return Err(BackendClientError::Protocol {
                reason: "room task page does not belong to the requested snapshot".to_owned(),
            });
        }
        let has_more = parse_boolean_header(response.headers(), ROOM_TASK_HAS_MORE_HEADER)?;
        let next_offset =
            parse_optional_usize_header(response.headers(), ROOM_TASK_NEXT_OFFSET_HEADER)?;
        if has_more != next_offset.is_some() {
            return Err(BackendClientError::Protocol {
                reason: format!(
                    "{ROOM_TASK_HAS_MORE_HEADER}=true requires {ROOM_TASK_NEXT_OFFSET_HEADER}, and false forbids it"
                ),
            });
        }
        let items: Vec<RoomTaskDto> = Self::decode_json_response(response).await?;
        if items.len() > normalized_limit {
            return Err(BackendClientError::Protocol {
                reason: format!(
                    "room tasks returned {} items for a normalized limit of {normalized_limit}",
                    items.len()
                ),
            });
        }
        if has_more && next_offset != Some(normalized_offset + items.len()) {
            return Err(BackendClientError::Protocol {
                reason: "room task continuation offset did not match the returned page".to_owned(),
            });
        }
        Ok(RoomTaskPageDto {
            snapshot: page_snapshot,
            items,
            has_more,
            next_offset,
        })
    }

    pub async fn create_room_task(
        &self,
        room_id: &str,
        task_id: &str,
        title: &str,
        description: Option<&str>,
        assigned_session_id: Option<&str>,
        assigned_session_incarnation_id: Option<&str>,
        due_at: Option<time::OffsetDateTime>,
    ) -> Result<RoomTaskDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/rooms/{}/tasks", encode_path_segment(room_id)),
            "backend.api",
        )?;
        let req = CreateRoomTaskRequest {
            task_id,
            title,
            description,
            assigned_session_id,
            assigned_session_incarnation_id,
            due_at,
        };
        self.send_json(self.authenticated(self.client.post(url)).json(&req))
            .await
    }

    pub async fn transition_room_task(
        &self,
        room_id: &str,
        task_id: &str,
        mutation_id: &uuid::Uuid,
        expected_task_revision: i64,
        to_status: &str,
        actor: Option<(&str, &str)>,
        result: Option<&str>,
    ) -> Result<RoomMutationResponseDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/{}/tasks/{}/status",
                encode_path_segment(room_id),
                encode_path_segment(task_id)
            ),
            "backend.api",
        )?;
        let (actor_session_id, actor_session_incarnation_id) = actor.unzip();
        let req = UpdateRoomTaskStatusRequest {
            request_id: mutation_id,
            expected_task_revision,
            status: to_status,
            actor_session_id,
            actor_session_incarnation_id,
            result,
        };
        let expected_result_revision = expected_task_revision.checked_add(1).ok_or_else(|| {
            BackendClientError::InvalidBackendData {
                field: "roomMutation.expectedTaskRevision".to_owned(),
                reason: "cannot advance beyond i64::MAX".to_owned(),
            }
        })?;
        let response = self
            .send_json(self.authenticated(self.client.post(url)).json(&req))
            .await?;
        validate_room_mutation_response(
            response,
            ExpectedRoomMutationResponse {
                request_id: mutation_id,
                operation: "tasks.transition",
                room_id,
                entity_id: task_id,
                result: to_status,
                revision: Some(expected_result_revision),
                assignee: actor.map_or(ExpectedRoomMutationAssignee::Any, |actor| {
                    ExpectedRoomMutationAssignee::Exact(Some(actor))
                }),
            },
        )
    }

    pub async fn assign_room_task(
        &self,
        room_id: &str,
        task_id: &str,
        mutation_id: &uuid::Uuid,
        expected_task_revision: i64,
        session_id: Option<&str>,
        session_incarnation_id: Option<&str>,
    ) -> Result<RoomMutationResponseDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/{}/tasks/{}/assign",
                encode_path_segment(room_id),
                encode_path_segment(task_id)
            ),
            "backend.api",
        )?;
        let req = AssignRoomTaskRequest {
            request_id: mutation_id,
            expected_task_revision,
            session_id,
            session_incarnation_id,
        };
        let expected_result_revision = expected_task_revision.checked_add(1).ok_or_else(|| {
            BackendClientError::InvalidBackendData {
                field: "roomMutation.expectedTaskRevision".to_owned(),
                reason: "cannot advance beyond i64::MAX".to_owned(),
            }
        })?;
        let response = self
            .send_json(self.authenticated(self.client.post(url)).json(&req))
            .await?;
        validate_room_mutation_response(
            response,
            ExpectedRoomMutationResponse {
                request_id: mutation_id,
                operation: "tasks.assign",
                room_id,
                entity_id: task_id,
                result: "Assigned",
                revision: Some(expected_result_revision),
                assignee: ExpectedRoomMutationAssignee::Exact(
                    session_id.zip(session_incarnation_id),
                ),
            },
        )
    }

    pub async fn fetch_room_mutation_receipt(
        &self,
        mutation_id: &uuid::Uuid,
        operation: crate::dto::rooms::RoomMutationOperationDto,
    ) -> Result<Option<RoomMutationReceiptDto>> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/rooms/mutations/{}/{}",
                encode_path_segment(operation.as_str()),
                encode_path_segment(&mutation_id.to_string())
            ),
            "backend.api",
        )?;
        let response = self
            .authenticated(self.client.get(url))
            .send()
            .await
            .map_err(BackendClientError::Http)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = check_status(response).await?;
        Self::decode_json_response(response).await.map(Some)
    }

    pub async fn fetch_my_sessions(&self) -> Result<Vec<SessionCardDto>> {
        let url = join_endpoint(self.base_url.as_ref(), "api/sessions/mine", "backend.api")?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_room_feed(&self, room_id: &str) -> Result<Vec<SessionCardDto>> {
        let mut items = Vec::new();
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        let mut pages_fetched = 0;

        loop {
            let mut url = join_endpoint(
                self.base_url.as_ref(),
                &format!("api/feed/room/{}", encode_path_segment(room_id)),
                "backend.api",
            )?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("limit", ROOM_FEED_PAGE_SIZE);
                if let Some(cursor) = cursor.as_deref() {
                    query.append_pair("cursor", cursor);
                }
            }

            let page: PaginatedSessionFeedDto = self
                .send_json(self.authenticated(self.client.get(url)))
                .await?;
            pages_fetched += 1;
            if page.items.len() > MAX_ROOM_FEED_ITEMS.saturating_sub(items.len()) {
                return Err(BackendClientError::Capacity {
                    resource: "room feed",
                    reason: format!("exceeds {MAX_ROOM_FEED_ITEMS} sessions"),
                });
            }
            items.extend(page.items);
            if !page.has_more {
                return Ok(items);
            }

            let next_cursor = page
                .next_cursor
                .filter(|value| !value.is_empty())
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: "room feed reported more pages without a next cursor".to_owned(),
                })?;
            if seen_cursors.contains(&next_cursor) {
                return Err(BackendClientError::Protocol {
                    reason: "room feed repeated a pagination cursor".to_owned(),
                });
            }
            if pages_fetched >= MAX_ROOM_FEED_PAGES {
                return Err(BackendClientError::Capacity {
                    resource: "room feed",
                    reason: format!("exceeds {MAX_ROOM_FEED_PAGES} pages"),
                });
            }
            seen_cursors.insert(next_cursor.clone());
            cursor = Some(next_cursor);
        }
    }

    pub async fn fetch_session_detail(&self, session_id: &str) -> Result<SessionDetailDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}", encode_path_segment(session_id)),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_session_creation_receipt(
        &self,
        session_id: &str,
        create_idempotency_key: &uuid::Uuid,
    ) -> Result<SessionCreationReceiptDto> {
        require_uuid_v7("create_idempotency_key", create_idempotency_key)?;
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/sessions/{}/creation-receipts/{}",
                encode_path_segment(session_id),
                encode_path_segment(&create_idempotency_key.to_string())
            ),
            "backend.api",
        )?;
        let receipt: SessionCreationReceiptDto = self
            .send_json(self.authenticated(self.client.get(url)))
            .await?;
        validate_session_creation_receipt(receipt, session_id, create_idempotency_key)
    }

    pub async fn claim_next_key_generation(
        &self,
        session_id: &str,
        incarnation_id: &uuid::Uuid,
        expected_current_generation: u32,
    ) -> Result<KeyGenerationClaimOutcome> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/sessions/{}/keys/claim-next-generation",
                encode_path_segment(session_id)
            ),
            "backend.api",
        )?;
        let request = ClaimNextKeyGenerationRequest {
            incarnation_id,
            expected_current_generation,
        };
        let dto: KeyGenerationClaimDto = self
            .send_json(self.authenticated(self.client.post(url)).json(&request))
            .await?;
        Ok(match dto.state {
            KeyGenerationClaimStateDto::Claimed => {
                KeyGenerationClaimOutcome::Claimed(dto.generation)
            }
            KeyGenerationClaimStateDto::GenerationChanged => {
                KeyGenerationClaimOutcome::GenerationChanged(dto.generation)
            }
        })
    }

    pub async fn fetch_current_key_generation(
        &self,
        session_id: &str,
        incarnation_id: &uuid::Uuid,
    ) -> Result<u32> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/sessions/{}/keys/current-generation",
                encode_path_segment(session_id)
            ),
            "backend.api",
        )?;
        url.query_pairs_mut()
            .append_pair("incarnationId", &incarnation_id.to_string());
        let dto: CurrentKeyGenerationDto = self
            .send_json(self.authenticated(self.client.get(url)))
            .await?;
        Ok(dto.current_generation)
    }

    pub async fn fetch_user_summary(&self, user_id: &str) -> Result<UserSummaryDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/users/{}", encode_path_segment(user_id)),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_user_identity(&self, user_id: &str) -> Result<UserIdentityBundleDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/users/{}/identity", encode_path_segment(user_id)),
            "backend.api",
        )?;
        let identity: UserIdentityBundleDto = self
            .send_json(self.authenticated(self.client.get(url)))
            .await?;
        if identity.user_id != user_id {
            return Err(BackendClientError::InvalidBackendData {
                field: "identity.userId".to_owned(),
                reason: "identity response does not match the requested user".to_owned(),
            });
        }
        Ok(identity)
    }

    pub async fn fetch_artifact_endorsements(
        &self,
        user_id: &str,
        digest: &str,
    ) -> Result<Vec<crate::artifact_endorsement::ArtifactEndorsement>> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/users/{}/artifact-endorsements",
                encode_path_segment(user_id)
            ),
            "backend.api",
        )?;
        url.query_pairs_mut().append_pair("digest", digest);
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn fetch_own_artifact_endorsements(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<crate::artifact_endorsement::ArtifactEndorsement>> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/artifact-endorsements",
            "backend.api",
        )?;
        url.query_pairs_mut()
            .append_pair("offset", &offset.to_string())
            .append_pair("limit", &limit.to_string());
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn put_artifact_endorsement(
        &self,
        endorsement: &crate::artifact_endorsement::PutArtifactEndorsement<'_>,
    ) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/artifact-endorsements",
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.put(url)).json(endorsement))
            .await
    }

    async fn decode_json_response<T: DeserializeOwned>(response: Response) -> Result<T> {
        let body =
            read_bounded_response_body(response, MAX_JSON_RESPONSE_BYTES, "backend JSON response")
                .await?;
        serde_json::from_slice(&body).map_err(BackendClientError::Json)
    }

    pub async fn create_session(&self, request: &CreateSessionRequest) -> Result<SessionDetailDto> {
        let url = join_endpoint(self.base_url.as_ref(), "api/sessions", "backend.api")?;
        self.send_json(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn grant_access(&self, session_id: &str, request: &GrantAccessRequest) -> Result<()> {
        require_uuid_v7("mutation_id", &request.mutation_id)?;
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}/access", encode_path_segment(session_id)),
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn revoke_access(
        &self,
        session_id: &str,
        incarnation_id: &uuid::Uuid,
        mutation_id: &uuid::Uuid,
        actor_user_id: &str,
    ) -> Result<()> {
        require_uuid_v7("mutation_id", mutation_id)?;
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/sessions/{}/access/{}",
                encode_path_segment(session_id),
                encode_path_segment(actor_user_id)
            ),
            "backend.api",
        )?;
        url.query_pairs_mut()
            .append_pair("expectedIncarnationId", &incarnation_id.to_string())
            .append_pair("mutationId", &mutation_id.to_string());
        self.send_empty(self.authenticated(self.client.delete(url)))
            .await
    }

    pub async fn leave_session_access(
        &self,
        session_id: &str,
        incarnation_id: &uuid::Uuid,
        mutation_id: &uuid::Uuid,
    ) -> Result<()> {
        require_uuid_v7("mutation_id", mutation_id)?;
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}/access/me", encode_path_segment(session_id)),
            "backend.api",
        )?;
        url.query_pairs_mut()
            .append_pair("expectedIncarnationId", &incarnation_id.to_string())
            .append_pair("mutationId", &mutation_id.to_string());
        self.send_empty(self.authenticated(self.client.delete(url)))
            .await
    }

    pub async fn get_session_access_mutation_receipt(
        &self,
        session_id: &str,
        incarnation_id: &uuid::Uuid,
        mutation_id: &uuid::Uuid,
    ) -> Result<SessionAccessMutationReceiptDto> {
        require_uuid_v7("mutation_id", mutation_id)?;
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/sessions/{}/access/mutations/{}",
                encode_path_segment(session_id),
                encode_path_segment(&mutation_id.to_string())
            ),
            "backend.api",
        )?;
        url.query_pairs_mut()
            .append_pair("expectedIncarnationId", &incarnation_id.to_string());
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn list_access(
        &self,
        session_id: &str,
        expected_incarnation_id: uuid::Uuid,
    ) -> Result<crate::dto::sessions::AccessGrantsDto> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}/access", encode_path_segment(session_id)),
            "backend.api",
        )?;
        url.query_pairs_mut().append_pair(
            "expectedIncarnationId",
            &expected_incarnation_id.to_string(),
        );
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn update_session(
        &self,
        session_id: &str,
        request: &UpdateSessionRequest,
    ) -> Result<SessionDetailDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}", encode_path_segment(session_id)),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.patch(url)).json(request))
            .await
    }

    pub async fn end_session(
        &self,
        session_id: &str,
        incarnation_id: &uuid::Uuid,
        mutation_id: &uuid::Uuid,
        attempt_id: &uuid::Uuid,
    ) -> Result<()> {
        require_uuid_v7("mutation_id", mutation_id)?;
        require_uuid_v7("attempt_id", attempt_id)?;
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}", encode_path_segment(session_id)),
            "backend.api",
        )?;
        url.query_pairs_mut()
            .append_pair("incarnationId", &incarnation_id.to_string());
        self.send_empty(
            self.authenticated(self.client.delete(url))
                .header("Idempotency-Key", mutation_id.to_string())
                .header("Kodosi-Attempt-Id", attempt_id.to_string()),
        )
        .await
    }

    pub async fn fetch_device_registration_challenge(
        &self,
    ) -> Result<DeviceRegistrationChallengeResponse> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/devices/challenge",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.post(url)))
            .await
    }

    pub async fn register_device_keys(&self, request: &RegisterDeviceRequest) -> Result<()> {
        let url = join_endpoint(self.base_url.as_ref(), "api/me/devices", "backend.api")?;
        self.send_empty(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn submit_device_list(&self, request: &SubmitDeviceListRequest) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/identity/device-list",
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn post_device_link_init(
        &self,
        request: &DeviceLinkInitRequest,
    ) -> Result<DeviceLinkInitResponse> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/devices/link/init",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn fetch_device_link_pending(&self, user_code: &str) -> Result<DeviceLinkPendingDto> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            "api/devices/link/pending",
            "backend.api",
        )?;
        url.query_pairs_mut().append_pair("userCode", user_code);
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn post_device_link_approve(&self, request: &DeviceLinkApproveRequest) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/devices/link/approve",
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn post_device_link_poll(
        &self,
        request: &DeviceLinkPollRequest,
    ) -> Result<DeviceLinkPollResponse> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/devices/link/poll",
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn post_device_link_ack(&self, request: &DeviceLinkAcknowledgeRequest) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/devices/link/ack",
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn delete_device_link_request(&self, user_code: &str) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/devices/link/requests/{}",
                encode_path_segment(user_code)
            ),
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.delete(url)))
            .await
    }

    pub async fn delete_my_identity(&self, request: Option<&IdentityResetRequest>) -> Result<()> {
        let url = join_endpoint(self.base_url.as_ref(), "api/me/identity", "backend.api")?;
        let builder = self.authenticated(self.client.delete(url));
        let builder = match request {
            Some(body) => builder.json(body),
            None => builder,
        };
        self.send_empty(builder).await
    }

    pub async fn store_session_key_blobs(
        &self,
        session_id: &str,
        request: &StoreKeyBlobsRequest,
    ) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}/keys", encode_path_segment(session_id)),
            "backend.api",
        )?;
        self.send_empty(self.authenticated(self.client.post(url)).json(request))
            .await
    }

    pub async fn fetch_authorized_devices(
        &self,
        session_id: &str,
    ) -> Result<Vec<AuthorizedDeviceDto>> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/sessions/{}/keys/authorized-devices",
                encode_path_segment(session_id)
            ),
            "backend.api",
        )?;
        self.send_json(self.authenticated(self.client.get(url)))
            .await
    }

    pub async fn upload_semantic_receipt(
        &self,
        proof: DeviceHttpProof<'_>,
        receipt: &SemanticReceiptDto,
    ) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/semantic-receipts",
            "backend.api",
        )?;
        let body = serde_json::to_vec(receipt)?;
        let body_sha256 = crate::crypto::sha256_hex(&body);
        let request = self
            .authenticated_device_request(reqwest::Method::POST, url, proof, &body_sha256)
            .await?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
        self.send_empty(request).await
    }

    pub async fn fetch_semantic_receipts(
        &self,
        proof: DeviceHttpProof<'_>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<crate::dto::semantic_receipts::SemanticReceiptPageDto> {
        let mut url = join_endpoint(
            self.base_url.as_ref(),
            "api/me/semantic-receipts",
            "backend.api",
        )?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &limit.clamp(1, 64).to_string());
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
        }
        let request = self
            .authenticated_device_request(reqwest::Method::GET, url, proof, EMPTY_BODY_SHA256)
            .await?;
        self.send_json(request).await
    }

    pub async fn acknowledge_semantic_receipt(
        &self,
        proof: DeviceHttpProof<'_>,
        request: &SemanticReceiptAckRequest,
    ) -> Result<()> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!(
                "api/me/semantic-receipts/{}/ack",
                encode_path_segment(&request.request_id.to_string())
            ),
            "backend.api",
        )?;
        let body = serde_json::to_vec(request)?;
        let body_sha256 = crate::crypto::sha256_hex(&body);
        let request = self
            .authenticated_device_request(reqwest::Method::POST, url, proof, &body_sha256)
            .await?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
        self.send_empty(request).await
    }

    pub async fn fetch_my_session_key(
        &self,
        session_id: &str,
        proof: DeviceHttpProof<'_>,
    ) -> Result<SessionKeyFetchDto> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("api/sessions/{}/keys/mine", encode_path_segment(session_id)),
            "backend.api",
        )?;
        let request = self
            .authenticated_device_request(reqwest::Method::GET, url, proof, EMPTY_BODY_SHA256)
            .await?;
        self.send_json(request).await
    }
}

#[derive(Debug, Clone, Copy)]
struct ExpectedRoomMutationResponse<'a> {
    request_id: &'a uuid::Uuid,
    operation: &'a str,
    room_id: &'a str,
    entity_id: &'a str,
    result: &'a str,
    revision: Option<i64>,
    assignee: ExpectedRoomMutationAssignee<'a>,
}

#[derive(Debug, Clone, Copy)]
enum ExpectedRoomMutationAssignee<'a> {
    Exact(Option<(&'a str, &'a str)>),
    Any,
}

fn validate_room_mutation_response(
    response: RoomMutationResponseDto,
    expected: ExpectedRoomMutationResponse<'_>,
) -> Result<RoomMutationResponseDto> {
    let parse_expected = |field: &'static str, value: &str| {
        uuid::Uuid::parse_str(value).map_err(|_| BackendClientError::InvalidBackendData {
            field: field.to_owned(),
            reason: "requested identifier must be a UUID".to_owned(),
        })
    };
    let expected_room_id = parse_expected("roomMutation.roomId", expected.room_id)?;
    let expected_entity_id = parse_expected("roomMutation.entityId", expected.entity_id)?;
    let assignee_matches = match expected.assignee {
        ExpectedRoomMutationAssignee::Exact(expected_assignee) => {
            let (expected_session, expected_incarnation) = expected_assignee.unzip();
            let expected_session = expected_session
                .map(|value| parse_expected("roomMutation.assigneeSessionId", value))
                .transpose()?;
            let expected_incarnation = expected_incarnation
                .map(|value| parse_expected("roomMutation.assigneeSessionIncarnationId", value))
                .transpose()?;
            response.assignee_session_id == expected_session
                && response.assignee_session_incarnation_id == expected_incarnation
        }
        ExpectedRoomMutationAssignee::Any => {
            response.assignee_session_id.is_some()
                == response.assignee_session_incarnation_id.is_some()
        }
    };
    if response.request_id != *expected.request_id
        || response.operation != expected.operation
        || response.room_id != expected_room_id
        || response.entity_id != expected_entity_id
        || response.result != expected.result
        || response.revision != expected.revision
        || !assignee_matches
    {
        return Err(BackendClientError::InvalidBackendData {
            field: "roomMutation.response".to_owned(),
            reason: "did not match the exact requested target".to_owned(),
        });
    }
    Ok(response)
}

fn validate_session_creation_receipt(
    receipt: SessionCreationReceiptDto,
    requested_session_id: &str,
    requested_key: &uuid::Uuid,
) -> Result<SessionCreationReceiptDto> {
    if receipt.session_id != requested_session_id {
        return Err(BackendClientError::Protocol {
            reason: "session creation receipt targeted a different session".to_owned(),
        });
    }
    if receipt.create_idempotency_key != *requested_key {
        return Err(BackendClientError::Protocol {
            reason: "session creation receipt targeted a different idempotency key".to_owned(),
        });
    }
    if receipt.incarnation_id.is_nil() || receipt.incarnation_id.get_version_num() != 7 {
        return Err(BackendClientError::Protocol {
            reason: "session creation receipt incarnationId must be a non-nil UUIDv7".to_owned(),
        });
    }
    if receipt.generation == 0 || receipt.protocol_version == 0 {
        return Err(BackendClientError::Protocol {
            reason: "session creation receipt generation and protocolVersion must be positive"
                .to_owned(),
        });
    }
    Ok(receipt)
}

fn require_uuid_v7(field: &str, value: &uuid::Uuid) -> Result<()> {
    if value.get_version_num() == 7 {
        return Ok(());
    }
    Err(BackendClientError::InvalidBackendData {
        field: field.to_owned(),
        reason: "must be UUIDv7".to_owned(),
    })
}

#[derive(Debug, Clone, Copy)]
enum RoomChatHeaders {
    Present {
        has_more: bool,
        next_since: Option<i64>,
        next_before: Option<i64>,
    },
    Missing,
}

fn normalize_room_chat_limit(limit: Option<i32>) -> usize {
    usize::try_from(
        limit
            .unwrap_or(ROOM_CHAT_DEFAULT_PAGE_SIZE)
            .clamp(1, ROOM_CHAT_MAX_PAGE_SIZE),
    )
    .unwrap_or(100)
}

fn parse_optional_i64_header(
    headers: &reqwest::header::HeaderMap,
    name: &'static str,
) -> Result<Option<i64>> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value >= 0)
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: format!("{name} must be a non-negative integer"),
                })
        })
        .transpose()
}

fn parse_optional_usize_header(
    headers: &reqwest::header::HeaderMap,
    name: &'static str,
) -> Result<Option<usize>> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: format!("{name} must be a non-negative integer"),
                })
        })
        .transpose()
}

fn parse_boolean_header(headers: &reqwest::header::HeaderMap, name: &'static str) -> Result<bool> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| match value {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
        .ok_or_else(|| BackendClientError::Protocol {
            reason: format!("{name} must be present and equal `true` or `false`"),
        })
}

fn parse_room_chat_headers(headers: &reqwest::header::HeaderMap) -> Result<RoomChatHeaders> {
    let has_more = headers.get(ROOM_CHAT_HAS_MORE_HEADER);
    let next_since = headers.get(ROOM_CHAT_NEXT_SINCE_HEADER);
    let next_before = headers.get(ROOM_CHAT_NEXT_BEFORE_HEADER);
    let Some(has_more) = has_more else {
        if next_since.is_some() || next_before.is_some() {
            return Err(BackendClientError::Protocol {
                reason: format!(
                    "room chat continuation was present without {ROOM_CHAT_HAS_MORE_HEADER}"
                ),
            });
        }
        return Ok(RoomChatHeaders::Missing);
    };
    let has_more = has_more
        .to_str()
        .ok()
        .and_then(|value| match value {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
        .ok_or_else(|| BackendClientError::Protocol {
            reason: format!("{ROOM_CHAT_HAS_MORE_HEADER} must be `true` or `false`"),
        })?;
    let next_since = next_since
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value >= 0)
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: format!("{ROOM_CHAT_NEXT_SINCE_HEADER} must be a non-negative integer"),
                })
        })
        .transpose()?;
    let next_before = next_before
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: format!("{ROOM_CHAT_NEXT_BEFORE_HEADER} must be a positive integer"),
                })
        })
        .transpose()?;
    let continuation_count = usize::from(next_since.is_some()) + usize::from(next_before.is_some());
    if (has_more && continuation_count != 1) || (!has_more && continuation_count != 0) {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "{ROOM_CHAT_HAS_MORE_HEADER}=true requires exactly one room chat continuation, and false forbids both"
            ),
        });
    }
    Ok(RoomChatHeaders::Present {
        has_more,
        next_since,
        next_before,
    })
}

fn resolve_room_chat_page(
    items: Vec<RoomChatMessageDto>,
    since: Option<i64>,
    before: Option<i64>,
    limit: usize,
    headers: RoomChatHeaders,
) -> Result<RoomChatPageDto> {
    if since.is_some() && before.is_some() {
        return Err(BackendClientError::Protocol {
            reason: "room chat request cannot combine `since` and `before`".to_owned(),
        });
    }
    if items.len() > limit {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "room chat returned {} items for a normalized limit of {limit}",
                items.len()
            ),
        });
    }
    let lower_bound = since.unwrap_or(0);
    if items
        .iter()
        .try_fold(lower_bound, |previous, item| {
            let inside_tail = before.is_none_or(|upper| item.seq < upper);
            (inside_tail && item.seq > previous).then_some(item.seq)
        })
        .is_none()
    {
        return Err(BackendClientError::Protocol {
            reason: "room chat sequences must be strictly increasing after `since`".to_owned(),
        });
    }

    let (has_more, next_since, next_before) = match headers {
        RoomChatHeaders::Present {
            has_more,
            next_since,
            next_before,
        } => {
            if since.is_some() && next_before.is_some() || before.is_some() && next_since.is_some()
            {
                return Err(BackendClientError::Protocol {
                    reason: "room chat continuation direction did not match the request".to_owned(),
                });
            }
            if has_more {
                let expected = if before.is_some() {
                    items.first().map(|item| item.seq)
                } else {
                    items.last().map(|item| item.seq)
                };
                if expected != next_since.or(next_before) {
                    return Err(BackendClientError::Protocol {
                        reason: "room chat continuation must equal the page boundary sequence"
                            .to_owned(),
                    });
                }
            }
            (has_more, next_since, next_before)
        }
        RoomChatHeaders::Missing if items.is_empty() => (false, None, None),
        RoomChatHeaders::Missing if items.len() == limit && before.is_none() => {
            (true, items.last().map(|item| item.seq), None)
        }
        RoomChatHeaders::Missing => {
            return Err(BackendClientError::Protocol {
                reason: format!(
                    "short non-empty room chat page omitted {ROOM_CHAT_HAS_MORE_HEADER}; completion cannot be determined safely"
                ),
            });
        }
    };

    Ok(RoomChatPageDto {
        items,
        has_more,
        next_since,
        next_before,
    })
}

#[derive(Debug, Deserialize)]
struct ProblemDetailsBody {
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

async fn check_status(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    if status == StatusCode::UNAUTHORIZED {
        return Err(BackendClientError::Unauthorized);
    }
    if status == StatusCode::NOT_FOUND {
        return Err(BackendClientError::NotFound);
    }
    if !status.is_client_error() {
        return Err(response
            .error_for_status()
            .map_or_else(BackendClientError::Http, |_| BackendClientError::Protocol {
                reason: format!("backend returned unexpected status {status}"),
            }));
    }
    let status_code = status.as_u16();

    let body_bytes = read_bounded_response_body(
        response,
        MAX_PROBLEM_RESPONSE_BYTES,
        "backend problem response",
    )
    .await?;
    let parsed = if body_bytes.is_empty() {
        None
    } else {
        serde_json::from_slice::<ProblemDetailsBody>(&body_bytes).ok()
    };
    let (code, detail) =
        parsed.map_or((None, None), |body| (body.code, body.detail.or(body.title)));
    Err(BackendClientError::HttpProblem {
        status: status_code,
        code,
        detail: detail.unwrap_or_else(|| format!("backend rejected request ({status_code})")),
    })
}

async fn read_bounded_response_body(
    response: Response,
    limit: usize,
    resource: &'static str,
) -> Result<BytesMut> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(BackendClientError::Capacity {
            resource,
            reason: format!("declared body exceeds {limit} bytes"),
        });
    }
    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(BackendClientError::Http)?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(BackendClientError::Capacity {
                resource,
                reason: format!("body exceeds {limit} bytes"),
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn device_proof_target_is_api_relative_and_preserves_query_order() {
        let client = test_client("https://backend.test/kodosi/".to_owned());
        let url = Url::parse(
            "https://backend.test/kodosi/api/me/semantic-receipts?limit=32&cursor=a%2Bb",
        )
        .unwrap();

        assert_eq!(
            client.api_relative_request_target(&url).unwrap(),
            "/api/me/semantic-receipts?limit=32&cursor=a%2Bb"
        );
        let reordered = Url::parse(
            "https://backend.test/kodosi/api/me/semantic-receipts?cursor=a%2Bb&limit=32",
        )
        .unwrap();
        assert_eq!(
            client.api_relative_request_target(&reordered).unwrap(),
            "/api/me/semantic-receipts?cursor=a%2Bb&limit=32"
        );
    }

    #[test]
    fn device_proof_target_preserves_slashless_endpoint_base_path() {
        let client = test_client("https://backend.test/api".to_owned());
        let url = Url::parse("https://backend.test/api/me/semantic-receipts").unwrap();

        assert_eq!(
            client.api_relative_request_target(&url).unwrap(),
            "/api/me/semantic-receipts"
        );
    }

    #[test]
    fn device_proof_target_rejects_paths_outside_configured_prefix() {
        let client = test_client("https://backend.test/kodosi/".to_owned());
        for url in [
            "https://backend.test/other/api/me/semantic-receipts",
            "https://backend.test/kodosix/api/me/semantic-receipts",
            "https://other.test/kodosi/api/me/semantic-receipts",
        ] {
            let url = Url::parse(url).unwrap();
            assert!(client.api_relative_request_target(&url).is_err(), "{url}");
        }
    }

    async fn empty_room_feed_server(
        page_count: usize,
        terminate_on_last_page: bool,
    ) -> (String, tokio::task::JoinHandle<usize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            for page_number in 1..=page_count {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut request = [0_u8; 2048];
                let _ = socket.read(&mut request).await.expect("request");
                let is_terminal = terminate_on_last_page && page_number == page_count;
                let (next_cursor, has_more) = if is_terminal {
                    ("null".to_owned(), false)
                } else {
                    (format!(r#""cursor-{page_number}""#), true)
                };
                let body =
                    format!(r#"{{"items":[],"nextCursor":{next_cursor},"hasMore":{has_more}}}"#);
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("response");
            }
            page_count
        });
        (format!("http://{address}/"), server)
    }

    fn test_client(api: String) -> BackendHttpClient {
        BackendHttpClient::new(&BackendClientConfig {
            api: Some(api),
            ..BackendClientConfig::default()
        })
        .expect("backend client")
    }

    async fn json_server(body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("request");
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .expect("response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    async fn response_server(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("request");
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .expect("response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    async fn room_chat_server(
        pagination_headers: &str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let pagination_headers = pagination_headers.to_owned();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 2048];
            let read = socket.read(&mut request).await.expect("request");
            let body = r#"[{"id":"message-7","roomId":"room-1","authorUserId":"user-1","authorSessionId":null,"authorKind":"Human","body":"ciphertext","recipientSessionIds":[],"recipientUserIds":[],"seq":7,"postedAt":"1970-01-01T00:00:00Z"}]"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\n{pagination_headers}Connection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .expect("response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    async fn bounded_room_bundle_server(
        transition_generation: i64,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let responses = vec![
                (
                    "Kodosi-Has-More: false\r\n".to_owned(),
                    r#"[{"id":"room-1","name":"Mission","slug":"mission","ownerUserId":"user-1","rosterGeneration":1,"rosterBody":"AQ==","rosterSignature":"Ag==","rosterSignerDeviceId":"device-1","rosterActivationProof":null,"admissionProofs":[],"rosterTransitions":[]}]"#.to_owned(),
                ),
                ("Kodosi-Has-More: false\r\n".to_owned(), "[]".to_owned()),
                (
                    "Kodosi-Has-More: false\r\n".to_owned(),
                    format!(
                        r#"[{{"generation":{transition_generation},"rosterBody":"AQ==","rosterSignature":"Ag==","rosterSignerDeviceId":"device-1","admissionInvitationId":null}}]"#
                    ),
                ),
            ];
            let mut requests = Vec::new();
            for (headers, body) in responses {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut request = [0_u8; 4096];
                let read = socket.read(&mut request).await.expect("request");
                requests.push(String::from_utf8_lossy(&request[..read]).into_owned());
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                             Content-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("response");
            }
            requests
        });
        (format!("http://{address}/"), server)
    }

    #[tokio::test]
    async fn room_catalog_hydrates_bounded_trust_bundle_surfaces() {
        let (api, server) = bounded_room_bundle_server(1).await;

        let rooms = test_client(api).fetch_rooms().await.expect("room bundle");

        assert_eq!(rooms.len(), 1);
        assert!(rooms[0].admission_proofs.is_empty());
        assert_eq!(rooms[0].roster_transitions.len(), 1);
        assert_eq!(rooms[0].roster_transitions[0].generation, 1);
        let requests = server.await.expect("server task");
        assert!(requests[0].starts_with("GET /api/rooms/?limit=3 "));
        assert!(requests[1].starts_with("GET /api/rooms/room-1/admission-proofs?limit=4 "));
        assert!(
            requests[2]
                .starts_with("GET /api/rooms/room-1/roster-transitions?afterGeneration=0&limit=8 ")
        );
    }

    #[tokio::test]
    async fn room_catalog_rejects_incomplete_roster_lineage() {
        let (api, server) = bounded_room_bundle_server(2).await;

        let error = test_client(api)
            .fetch_rooms()
            .await
            .expect_err("generation two cannot appear without generation one");

        assert!(matches!(error, BackendClientError::Protocol { .. }));
        assert_eq!(server.await.expect("server task").len(), 3);
    }

    #[tokio::test]
    async fn creation_receipt_lookup_sends_path_safe_ids_and_validates_target() {
        let session_id = "session/with space";
        let key = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
            .expect("idempotency UUID");
        let (api, server) = json_server(
            r#"{"sessionId":"session/with space","createIdempotencyKey":"01900000-0000-7000-8000-000000000002","incarnationId":"01900000-0000-7000-8000-000000000003","generation":2,"protocolVersion":2}"#,
        )
        .await;

        let receipt = test_client(api)
            .fetch_session_creation_receipt(session_id, &key)
            .await
            .expect("matching receipt");

        assert_eq!(receipt.session_id, session_id);
        let request = server.await.expect("server");
        assert!(request.starts_with(
            "GET /api/sessions/session%2Fwith%20space/creation-receipts/01900000-0000-7000-8000-000000000002 "
        ));
    }

    #[tokio::test]
    async fn creation_receipt_lookup_rejects_non_v7_before_dispatch() {
        let error = test_client("http://127.0.0.1:1/".to_owned())
            .fetch_session_creation_receipt("session", &uuid::Uuid::from_u128(1))
            .await
            .expect_err("non-v7 must fail before I/O");
        std::assert_matches!(
            error,
            BackendClientError::InvalidBackendData { ref field, .. }
                if field == "create_idempotency_key"
        );
    }

    #[tokio::test]
    async fn creation_receipt_lookup_rejects_hostile_response_binding() {
        let key = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
            .expect("idempotency UUID");
        for body in [
            r#"{"sessionId":"other","createIdempotencyKey":"01900000-0000-7000-8000-000000000002","incarnationId":"01900000-0000-7000-8000-000000000003","generation":2,"protocolVersion":2}"#,
            r#"{"sessionId":"session","createIdempotencyKey":"01900000-0000-7000-8000-000000000004","incarnationId":"01900000-0000-7000-8000-000000000003","generation":2,"protocolVersion":2}"#,
            r#"{"sessionId":"session","createIdempotencyKey":"01900000-0000-7000-8000-000000000002","incarnationId":"00000000-0000-0000-0000-000000000000","generation":2,"protocolVersion":2}"#,
        ] {
            let (api, server) = json_server(body).await;
            let error = test_client(api)
                .fetch_session_creation_receipt("session", &key)
                .await
                .expect_err("hostile binding must fail closed");
            assert!(matches!(error, BackendClientError::Protocol { .. }));
            server.await.expect("server");
        }
    }

    #[tokio::test]
    async fn task_pages_require_and_echo_exact_snapshot() {
        let token = "a".repeat(64);
        for supplied_header in [
            None,
            Some("bad".to_owned()),
            Some("b".repeat(64)),
            Some(token.clone()),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let response_header = supplied_header.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 2048];
                let read = socket.read(&mut request).await.unwrap();
                let snapshot_header = response_header.map_or_else(String::new, |value| {
                    format!("Kodosi-Task-Snapshot: {value}\r\n")
                });
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nKodosi-Has-More: false\r\n{snapshot_header}Connection: close\r\n\r\n[]").as_bytes()).await.unwrap();
                String::from_utf8_lossy(&request[..read]).into_owned()
            });
            let result = test_client(format!("http://{address}/"))
                .fetch_room_tasks("room", None, None, Some(1), Some(500), Some(&token))
                .await;
            if supplied_header.as_deref() == Some(token.as_str()) {
                assert_eq!(result.unwrap().snapshot, token);
            } else {
                assert!(matches!(result, Err(BackendClientError::Protocol { .. })));
            }
            assert!(server.await.unwrap().contains(&format!("snapshot={token}")));
        }
    }

    #[tokio::test]
    async fn room_chat_parses_short_page_continuation_headers() {
        let (api, server) =
            room_chat_server("Kodosi-Has-More: true\r\nKodosi-Next-Since: 7\r\n").await;

        let page = test_client(api)
            .fetch_room_chat("room-1", Some(0), Some(200))
            .await
            .expect("room chat page");

        assert_eq!(page.items.len(), 1);
        assert!(page.has_more);
        assert_eq!(page.next_since, Some(7));
        let _request = server.await.expect("server task");
    }

    #[tokio::test]
    async fn key_generation_claim_sends_incarnation_and_expected_fence() {
        let (api, server) = json_server(r#"{"state":"GenerationChanged","generation":8}"#).await;

        let outcome = test_client(api)
            .claim_next_key_generation(
                "01900000-0000-7000-8000-000000000001",
                &uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
                    .expect("incarnation UUID"),
                7,
            )
            .await
            .expect("claim response");

        assert_eq!(outcome, KeyGenerationClaimOutcome::GenerationChanged(8));
        let request = server.await.expect("server task");
        assert!(request.starts_with(
            "POST /api/sessions/01900000-0000-7000-8000-000000000001/keys/claim-next-generation "
        ));
        assert!(request.contains(r#""incarnationId":"01900000-0000-7000-8000-000000000002""#));
        assert!(request.contains(r#""expectedCurrentGeneration":7"#));
    }

    #[tokio::test]
    async fn current_generation_query_sends_incarnation() {
        let (api, server) = json_server(r#"{"currentGeneration":8}"#).await;

        let generation = test_client(api)
            .fetch_current_key_generation(
                "01900000-0000-7000-8000-000000000001",
                &uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
                    .expect("incarnation UUID"),
            )
            .await
            .expect("current generation");

        assert_eq!(generation, 8);
        let request = server.await.expect("server task");
        assert!(request.starts_with(
            "GET /api/sessions/01900000-0000-7000-8000-000000000001/keys/current-generation?incarnationId=01900000-0000-7000-8000-000000000002 "
        ));
    }

    #[tokio::test]
    async fn session_delete_sends_exact_v5_mutation_headers() {
        let (api, server) = response_server("204 No Content", "").await;
        let incarnation_id = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
            .expect("incarnation UUID");
        let mutation_id =
            uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000003").expect("mutation UUIDv7");
        let attempt_id =
            uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000004").expect("attempt UUIDv7");

        test_client(api)
            .end_session(
                "01900000-0000-7000-8000-000000000001",
                &incarnation_id,
                &mutation_id,
                &attempt_id,
            )
            .await
            .expect("conditional idempotent delete");

        let request = server.await.expect("server task");
        assert!(request.starts_with(
            "DELETE /api/sessions/01900000-0000-7000-8000-000000000001?incarnationId=01900000-0000-7000-8000-000000000002 "
        ));
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("\r\nidempotency-key: 01900000-0000-7000-8000-000000000003\r\n"));
        assert!(lower.contains("\r\nkodosi-attempt-id: 01900000-0000-7000-8000-000000000004\r\n"));
    }

    #[tokio::test]
    async fn session_delete_rejects_non_v7_ids_before_dispatch() {
        let v4 = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("UUIDv4");
        let v7 = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000003").expect("UUIDv7");
        let client = test_client("http://127.0.0.1:9/".to_owned());

        for (mutation_id, attempt_id, field) in
            [(&v4, &v7, "mutation_id"), (&v7, &v4, "attempt_id")]
        {
            let error = client
                .end_session("session", &uuid::Uuid::nil(), mutation_id, attempt_id)
                .await
                .expect_err("non-v7 ID must be rejected before I/O");
            std::assert_matches!(
                error,
                BackendClientError::InvalidBackendData { field: ref actual, .. }
                    if actual == field
            );
        }
    }

    #[tokio::test]
    async fn session_delete_preserves_not_found_as_terminal_outcome() {
        let (api, server) = response_server("404 Not Found", "").await;
        let v7 = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000003").expect("UUIDv7");

        let error = test_client(api)
            .end_session("session", &uuid::Uuid::nil(), &v7, &v7)
            .await
            .expect_err("404 remains typed NotFound");

        assert!(matches!(error, BackendClientError::NotFound));
        let _request = server.await.expect("server task");
    }

    #[tokio::test]
    async fn revoke_and_dismiss_send_mutation_identity_and_incarnation_condition() {
        let incarnation_id = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
            .expect("incarnation UUID");
        let mutation_id =
            uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000004").expect("mutation UUID");
        let (revoke_api, revoke_server) = json_server("").await;
        test_client(revoke_api)
            .revoke_access(
                "01900000-0000-7000-8000-000000000001",
                &incarnation_id,
                &mutation_id,
                "01900000-0000-7000-8000-000000000003",
            )
            .await
            .expect("conditional revoke");
        let revoke_request = revoke_server.await.expect("revoke server task");
        assert!(revoke_request.starts_with(
            "DELETE /api/sessions/01900000-0000-7000-8000-000000000001/access/01900000-0000-7000-8000-000000000003?expectedIncarnationId=01900000-0000-7000-8000-000000000002&mutationId=01900000-0000-7000-8000-000000000004 "
        ));

        let (dismiss_api, dismiss_server) = json_server("").await;
        test_client(dismiss_api)
            .leave_session_access(
                "01900000-0000-7000-8000-000000000001",
                &incarnation_id,
                &mutation_id,
            )
            .await
            .expect("conditional dismissal");
        let dismiss_request = dismiss_server.await.expect("dismiss server task");
        assert!(dismiss_request.starts_with(
            "DELETE /api/sessions/01900000-0000-7000-8000-000000000001/access/me?expectedIncarnationId=01900000-0000-7000-8000-000000000002&mutationId=01900000-0000-7000-8000-000000000004 "
        ));
    }

    #[tokio::test]
    async fn access_mutation_receipt_lookup_binds_exact_target() {
        let mutation_id =
            uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000004").expect("mutation UUID");
        let incarnation_id = uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
            .expect("incarnation UUID");
        let response = r#"{
            "mutationId":"01900000-0000-7000-8000-000000000004",
            "sessionId":"01900000-0000-7000-8000-000000000001",
            "incarnationId":"01900000-0000-7000-8000-000000000002",
            "kind":"grant",
            "targetUserId":"01900000-0000-7000-8000-000000000003",
            "accessLevel":"Inject",
            "requestedExpiresAt":"2026-08-14T00:00:00Z"
        }"#;
        let (api, server) = json_server(response).await;

        let receipt = test_client(api)
            .get_session_access_mutation_receipt(
                "01900000-0000-7000-8000-000000000001",
                &incarnation_id,
                &mutation_id,
            )
            .await
            .expect("receipt should decode");

        assert_eq!(receipt.mutation_id, mutation_id);
        assert_eq!(
            receipt.kind,
            crate::dto::sessions::SessionAccessMutationKindDto::Grant
        );
        assert_eq!(
            receipt.access_level,
            Some(kodosi_domain::permissions::AccessLevel::Inject)
        );
        let request = server.await.expect("receipt server task");
        assert!(request.starts_with(
            "GET /api/sessions/01900000-0000-7000-8000-000000000001/access/mutations/01900000-0000-7000-8000-000000000004?expectedIncarnationId=01900000-0000-7000-8000-000000000002 "
        ));
    }

    #[tokio::test]
    async fn room_chat_tail_uses_before_cursor_and_validates_boundary() {
        let (api, server) =
            room_chat_server("Kodosi-Has-More: true\r\nKodosi-Next-Before: 7\r\n").await;

        let page = test_client(api)
            .fetch_room_chat_tail("room-1", None, Some(1000))
            .await
            .expect("tail continuation should decode");

        assert!(page.has_more);
        assert_eq!(page.next_since, None);
        assert_eq!(page.next_before, Some(7));
        let request = server.await.expect("server task");
        assert!(
            request
                .starts_with("GET /api/rooms/room-1/chat?before=9223372036854775807&limit=1000 ")
        );
    }

    #[tokio::test]
    async fn short_legacy_room_chat_page_fails_instead_of_silently_truncating() {
        let (api, server) = room_chat_server("").await;

        let error = test_client(api)
            .fetch_room_chat("room-1", Some(0), Some(200))
            .await
            .expect_err("short page without completion metadata is ambiguous");

        std::assert_matches!(
            error,
            BackendClientError::Protocol { ref reason }
                if reason.contains("completion cannot be determined safely")
        );
        let _request = server.await.expect("server task");
    }

    #[tokio::test]
    async fn full_legacy_room_chat_page_infers_safe_conservative_continuation() {
        let (api, server) = room_chat_server("").await;

        let page = test_client(api)
            .fetch_room_chat("room-1", Some(0), Some(1))
            .await
            .expect("a full legacy page can safely request one more page");

        assert!(page.has_more);
        assert_eq!(page.next_since, Some(7));
        let _request = server.await.expect("server task");
    }

    #[tokio::test]
    async fn unique_cursor_empty_room_feed_pages_are_bounded() {
        let (api, server) = empty_room_feed_server(MAX_ROOM_FEED_PAGES, false).await;
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            test_client(api).fetch_room_feed("room-1"),
        )
        .await
        .expect("room feed pagination should terminate")
        .expect_err("room feed exceeding the page bound must fail closed");

        std::assert_matches!(
            error,
            BackendClientError::Capacity {
                resource: "room feed",
                ref reason,
            } if reason == &format!("exceeds {MAX_ROOM_FEED_PAGES} pages")
        );
        assert_eq!(
            server.await.expect("server task"),
            MAX_ROOM_FEED_PAGES,
            "the client must not request a page beyond the bound"
        );
    }

    #[tokio::test]
    async fn room_feed_accepts_exact_page_bound() {
        let (api, server) = empty_room_feed_server(MAX_ROOM_FEED_PAGES, true).await;
        let items = test_client(api)
            .fetch_room_feed("room-1")
            .await
            .expect("a terminal page at the bound should succeed");

        assert!(items.is_empty());
        assert_eq!(server.await.expect("server task"), MAX_ROOM_FEED_PAGES);
    }

    #[tokio::test]
    async fn identity_response_is_bound_to_the_requested_user() {
        const IDENTITY: &str = r#"{
            "userId":"peer-b",
            "identityRevision":1,
            "identityIncarnationId":"01900000-0000-7000-8000-000000000001",
            "deviceList":{"body":"signed-list","signature":"signature"},
            "devices":[]
        }"#;
        let (api, server) = json_server(IDENTITY).await;
        let identity = test_client(api)
            .fetch_user_identity("peer-b")
            .await
            .expect("matching identity");
        assert_eq!(identity.user_id, "peer-b");
        assert!(
            server
                .await
                .expect("server")
                .starts_with("GET /api/users/peer-b/identity ")
        );

        let (api, server) = json_server(IDENTITY).await;
        let error = test_client(api)
            .fetch_user_identity("peer-a")
            .await
            .expect_err("another user's valid bundle must not reach pin verification");
        std::assert_matches!(
            error,
            BackendClientError::InvalidBackendData { ref field, .. }
                if field == "identity.userId"
        );
        assert!(
            server
                .await
                .expect("server")
                .starts_with("GET /api/users/peer-a/identity ")
        );
    }

    #[tokio::test]
    async fn oversized_problem_body_is_rejected_before_buffering() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await.expect("request");
            let declared = MAX_PROBLEM_RESPONSE_BYTES + 1;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Length: {declared}\r\n\
                         Connection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .expect("response headers");
        });

        let response = reqwest::Client::new()
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect("response");
        let error = check_status(response)
            .await
            .expect_err("oversized problem response must be bounded");
        std::assert_matches!(
            error,
            BackendClientError::Capacity {
                resource: "backend problem response",
                ..
            }
        );
        server.await.expect("server task");
    }
}
