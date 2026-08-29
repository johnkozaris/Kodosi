use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreKeyBlobsRequest {
    pub incarnation_id: Uuid,
    pub blobs: Vec<KeyBlobEntry>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaimNextKeyGenerationRequest<'a> {
    pub incarnation_id: &'a Uuid,
    pub expected_current_generation: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBlobEntry {
    pub recipient_device_id: String,
    pub encrypted_session_key: String,
    pub sender_device_id: String,
    pub key_generation: u32,
    pub issued_at_ms: u64,
    pub signature: String,
    pub signature_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionKeyBlobDto {
    pub incarnation_id: Uuid,
    pub incarnation_protocol_version: u32,
    pub encrypted_session_key: String,
    pub sender_device_id: String,
    pub signature: Option<String>,
    pub key_generation: u32,
    pub issued_at_ms: u64,
    pub signature_version: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum SessionKeyFetchStateDto {
    Ready,
    PendingDistribution,
    UnknownDevice,
    SessionNotLive,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionKeyFetchDto {
    pub state: SessionKeyFetchStateDto,
    pub key_blob: Option<SessionKeyBlobDto>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) enum KeyGenerationClaimStateDto {
    Claimed,
    GenerationChanged,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyGenerationClaimDto {
    pub state: KeyGenerationClaimStateDto,
    pub generation: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentKeyGenerationDto {
    pub current_generation: u32,
}

#[cfg(test)]
mod tests {
    use super::{
        ClaimNextKeyGenerationRequest, KeyGenerationClaimDto, KeyGenerationClaimStateDto,
        SessionKeyFetchStateDto, StoreKeyBlobsRequest,
    };

    #[test]
    fn session_key_fetch_states_match_backend_contract() {
        let cases = [
            ("\"Ready\"", SessionKeyFetchStateDto::Ready),
            (
                "\"PendingDistribution\"",
                SessionKeyFetchStateDto::PendingDistribution,
            ),
            ("\"UnknownDevice\"", SessionKeyFetchStateDto::UnknownDevice),
            (
                "\"SessionNotLive\"",
                SessionKeyFetchStateDto::SessionNotLive,
            ),
        ];

        for (wire, expected) in cases {
            let parsed = serde_json::from_str::<SessionKeyFetchStateDto>(wire)
                .unwrap_or_else(|error| panic!("{wire} should deserialize: {error}"));
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn removed_access_denied_wire_state_fails_closed() {
        let result = serde_json::from_str::<SessionKeyFetchStateDto>("\"AccessDenied\"");

        assert!(result.is_err());
    }

    #[test]
    fn key_mutation_requests_carry_session_incarnation() {
        let claim = serde_json::to_value(ClaimNextKeyGenerationRequest {
            incarnation_id: &uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
                .expect("incarnation UUID"),
            expected_current_generation: 7,
        })
        .expect("claim request should serialize");
        assert_eq!(
            claim["incarnationId"],
            "01900000-0000-7000-8000-000000000002"
        );
        assert_eq!(claim["expectedCurrentGeneration"], 7);

        let publication = serde_json::to_value(StoreKeyBlobsRequest {
            incarnation_id: uuid::Uuid::parse_str("01900000-0000-7000-8000-000000000002")
                .expect("incarnation UUID"),
            blobs: Vec::new(),
        })
        .expect("publication request should serialize");
        assert_eq!(
            publication["incarnationId"],
            "01900000-0000-7000-8000-000000000002"
        );
    }

    #[test]
    fn generation_changed_claim_response_is_typed() {
        let response: KeyGenerationClaimDto =
            serde_json::from_str(r#"{"state":"GenerationChanged","generation":8}"#)
                .expect("claim response should deserialize");

        assert_eq!(
            response.state,
            KeyGenerationClaimStateDto::GenerationChanged
        );
        assert_eq!(response.generation, 8);
    }
}
