#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Deserialize;

use kodosi_backend_client::user_events::DiscoverySurface;
use kodosi_runtime::protocol_limits;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRelayAuthority {
    messages: BTreeMap<String, RelayMessageAuthority>,
    session_capability_bits: BTreeMap<String, u32>,
    session_capability_masks: BTreeMap<String, u32>,
    unknown_message_policy: String,
    malformed_payload_policy: String,
    semantic_send: SemanticSendAuthority,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SemanticSendAuthority {
    canonical_signed_encoding: String,
    semantic_request_aad_fields: Vec<String>,
    semantic_request_signature_fields: Vec<String>,
    semantic_request_hkdf_info: String,
    semantic_cancel_signature_fields: Vec<String>,
    semantic_receipt_signature_fields: Vec<String>,
    semantic_receipt_ack_signature_fields: Vec<String>,
}

#[test]
fn relay_semantic_encoding_matches_crypto_contract() {
    let authority = load_session_relay_authority();
    let semantic = authority.semantic_send;
    assert_eq!(
        semantic.canonical_signed_encoding,
        "domain tag bytes followed by each field as uint32 big-endian UTF-8 byte length then exact UTF-8 bytes; UUID fields use lowercase hyphenated D form; binary nonce and ciphertext fields use uint32 big-endian byte length then raw bytes"
    );
    assert_eq!(
        semantic.semantic_request_aad_fields,
        [
            "sessionId",
            "incarnationId",
            "requestId",
            "mode",
            "payloadSha256",
            "requesterUserId",
            "requesterDeviceId",
        ]
    );
    let mut request_signature = semantic.semantic_request_aad_fields.clone();
    request_signature.extend(["nonce".to_owned(), "ciphertext".to_owned()]);
    assert_eq!(
        semantic.semantic_request_signature_fields,
        request_signature
    );
    assert_eq!(
        semantic.semantic_request_hkdf_info,
        "kodosi-semantic-request-key-v1"
    );
    assert_eq!(
        semantic.semantic_cancel_signature_fields,
        semantic.semantic_request_aad_fields
    );
    assert_eq!(
        semantic.semantic_receipt_signature_fields,
        [
            "sessionId",
            "incarnationId",
            "requestId",
            "mode",
            "payloadSha256",
            "outcome",
            "requesterUserId",
            "requesterDeviceId",
            "ownerUserId",
            "ownerDeviceId",
        ]
    );
    assert_eq!(
        semantic.semantic_receipt_ack_signature_fields,
        [
            "sessionId",
            "incarnationId",
            "requestId",
            "requesterUserId",
            "requesterDeviceId",
        ]
    );
}

#[test]
fn relay_capability_positions_and_roles_match_production() {
    use kodosi_domain::permissions::{AccessLevel, SessionCapabilities};

    let authority = load_session_relay_authority();
    let positions = [
        ("view", SessionCapabilities::VIEW),
        ("suggest", SessionCapabilities::SUGGEST),
        ("sendInput", SessionCapabilities::SEND_INPUT),
        ("approveDeny", SessionCapabilities::APPROVE_DENY),
        ("resize", SessionCapabilities::RESIZE),
        ("focus", SessionCapabilities::FOCUS),
        ("stop", SessionCapabilities::STOP),
    ]
    .into_iter()
    .map(|(name, mask)| {
        assert!(mask.is_power_of_two(), "{name} must remain a single bit");
        (name.to_owned(), mask.trailing_zeros())
    })
    .collect::<BTreeMap<_, _>>();
    assert_eq!(authority.session_capability_bits, positions);

    let role_masks = [
        (
            "Approve",
            SessionCapabilities::from_access(AccessLevel::Approve, false).0,
        ),
        (
            "Inject",
            SessionCapabilities::from_access(AccessLevel::Inject, false).0,
        ),
        (
            "Owner",
            SessionCapabilities::from_access(AccessLevel::View, true).0,
        ),
        (
            "Suggest",
            SessionCapabilities::from_access(AccessLevel::Suggest, false).0,
        ),
        (
            "View",
            SessionCapabilities::from_access(AccessLevel::View, false).0,
        ),
    ]
    .into_iter()
    .map(|(name, mask)| (name.to_owned(), u32::from(mask)))
    .collect::<BTreeMap<_, _>>();
    assert_eq!(authority.session_capability_masks, role_masks);
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayMessageAuthority {
    direction: String,
    #[serde(default)]
    authorization: Option<String>,
    ordering: String,
    #[serde(default)]
    counter_domain: Option<String>,
    #[serde(default)]
    required_fields: Vec<String>,
    #[serde(default)]
    optional_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserEventsAuthority {
    outgoing_types: Vec<String>,
    discovery_surfaces: Vec<String>,
    unknown_message_policy: String,
    malformed_payload_policy: String,
}

fn protocol_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.push("protocol");
    path
}

fn load_session_relay_authority() -> SessionRelayAuthority {
    let path = protocol_dir().join("session-relay-authority.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("session-relay-authority.json must deserialize")
}

fn load_user_events_authority() -> UserEventsAuthority {
    let path = protocol_dir().join("user-events-authority.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("user-events-authority.json must deserialize")
}

#[test]
fn relay_pending_permissions_authority_is_explicit() {
    let authority = load_session_relay_authority();
    let pending = authority
        .messages
        .get("permission.pendingSnapshot")
        .expect("missing pending-permissions authority");
    assert_eq!(pending.direction, "hostToBackendToParticipant");
    assert_eq!(
        pending.authorization.as_deref(),
        Some("forward only to owner participants or roles with Suggest, SendInput, or ApproveDeny")
    );
    assert!(
        pending
            .ordering
            .contains("snapshotGeneration strictly increases")
    );
    assert!(
        pending
            .counter_domain
            .as_deref()
            .is_some_and(|domain| domain.contains("pendingPermissions HKDF subkey"))
    );
}

#[test]
fn relay_resize_declares_signed_pixel_geometry_end_to_end() {
    let authority = load_session_relay_authority();
    for message_type in ["participant.resize", "host.resize"] {
        let message = authority
            .messages
            .get(message_type)
            .unwrap_or_else(|| panic!("missing {message_type} authority"));
        for field in [
            "widthPixels",
            "heightPixels",
            "cellWidthPixels",
            "cellHeightPixels",
        ] {
            assert!(
                message
                    .optional_fields
                    .iter()
                    .any(|candidate| candidate == field),
                "{message_type} must declare optional {field}"
            );
            assert!(
                !message
                    .required_fields
                    .iter()
                    .any(|candidate| candidate == field),
                "{message_type} must not require nullable {field}"
            );
        }
    }
}

#[test]
fn relay_resize_is_owner_only_end_to_end() {
    let authority = load_session_relay_authority();
    for message_type in ["participant.resize", "host.resize"] {
        assert_eq!(
            authority
                .messages
                .get(message_type)
                .and_then(|message| message.authorization.as_deref()),
            Some("ownerOnly"),
            "{message_type} must remain owner-only",
        );
    }
}

const SHARED_CONSUMED_TYPES: &[&str] = &[
    "participant.semanticReceipt",
    "participant.accepted",
    "action.result",
    "session.status",
    "session.ended",
    "session.accessRevoked",
    "key.rotation",
    "term.semanticCheckpoint",
    "term.rawBatch",
    "term.presentation",
    "permission.pendingSnapshot",
];

#[test]
fn rust_host_consumed_types_match_session_relay_authority() {
    let authority = load_session_relay_authority();
    assert_eq!(authority.unknown_message_policy, "terminalClose");
    assert_eq!(authority.malformed_payload_policy, "terminalClose");
    let host_incoming = message_types_by_direction(&authority, &["backendToHost"]);
    let consumed = kodosi_backend_client::relay::HostIncomingMessageType::ALL
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    assert_eq!(
        consumed, host_incoming,
        "the production host relay dispatch catalog must exactly match backendToHost authority"
    );
}

#[test]
fn rust_shared_consumed_types_match_session_relay_authority() {
    let authority = load_session_relay_authority();
    assert_eq!(authority.unknown_message_policy, "terminalClose");
    assert_eq!(authority.malformed_payload_policy, "terminalClose");
    let shared = message_types_by_direction(
        &authority,
        &["backendToParticipant", "hostToBackendToParticipant"],
    );

    for message_type in SHARED_CONSUMED_TYPES {
        assert!(
            shared.contains(message_type),
            "Rust consumes \"{message_type}\" but the session-relay manifest \
            doesn't list it for participant delivery. The backend is emitting a \
            type this runtime never knew about, or the manifest is out of sync."
        );
    }

    for manifest_type in &shared {
        assert!(
            SHARED_CONSUMED_TYPES.contains(manifest_type),
            "session-relay manifest lists participant-delivered {manifest_type}, \
            but no Rust consumer is declared for it in SHARED_CONSUMED_TYPES."
        );
    }
}

fn message_types_by_direction<'a>(
    authority: &'a SessionRelayAuthority,
    directions: &[&str],
) -> BTreeSet<&'a str> {
    authority
        .messages
        .iter()
        .filter(|(_, spec)| directions.contains(&spec.direction.as_str()))
        .map(|(message_type, _)| message_type.as_str())
        .collect()
}

const HOST_OUTBOUND_TYPES: &[&str] = &[
    "host.actionResult",
    "host.semanticReceipt",
    "host.hello",
    "host.heartbeat",
    "host.end",
    "host.fenceAck",
];

const PARTICIPANT_OUTBOUND_TYPES: &[&str] = &[
    "participant.semanticSend",
    "participant.semanticCancel",
    "participant.semanticReceiptAck",
    "participant.join",
    "participant.heartbeat",
    "participant.suggest",
    "participant.inject",
    "participant.resize",
    "participant.focusChanged",
    "participant.permissionDecision",
    "participant.stop",
    "participant.interrupt",
];

const HOST_FORWARDED_OUTBOUND_TYPES: &[&str] = &[
    "key.rotation",
    "term.semanticCheckpoint",
    "term.rawBatch",
    "term.presentation",
    "permission.pendingSnapshot",
];

#[test]
fn rust_host_outbound_types_match_session_relay_authority() {
    let authority = load_session_relay_authority();
    let manifest = message_types_by_direction(&authority, &["hostToBackend"]);

    for message_type in HOST_OUTBOUND_TYPES {
        assert!(
            manifest.contains(message_type),
            "Rust host emits \"{message_type}\" but the session-relay manifest \
            doesn't list it as hostToBackend. Backend renamed/dropped it, or \
            the manifest is stale."
        );
    }

    for manifest_type in &manifest {
        assert!(
            HOST_OUTBOUND_TYPES.contains(manifest_type),
            "session-relay manifest lists hostToBackend={manifest_type}, but \
            Rust does not declare it as an outbound type. Add a producer in \
            backend_client/relay/wire.rs and update HOST_OUTBOUND_TYPES."
        );
    }
}

#[test]
fn rust_participant_outbound_types_match_session_relay_authority() {
    let authority = load_session_relay_authority();
    let manifest = message_types_by_direction(&authority, &["participantToBackend"]);

    for message_type in PARTICIPANT_OUTBOUND_TYPES {
        assert!(
            manifest.contains(message_type),
            "Rust participant client emits \"{message_type}\" but the manifest \
            doesn't list it as participantToBackend. Backend renamed/dropped \
            it, or the manifest is stale."
        );
    }

    for manifest_type in &manifest {
        assert!(
            PARTICIPANT_OUTBOUND_TYPES.contains(manifest_type),
            "session-relay manifest lists participantToBackend={manifest_type}, \
            but Rust does not declare it as an outbound type. Add a producer \
            in backend_client/session_relay/ws.rs and update \
            PARTICIPANT_OUTBOUND_TYPES."
        );
    }
}

#[test]
fn rust_host_forwarded_outbound_types_match_session_relay_authority() {
    let authority = load_session_relay_authority();
    let manifest = message_types_by_direction(&authority, &["hostToBackendToParticipant"]);

    for message_type in HOST_FORWARDED_OUTBOUND_TYPES {
        assert!(
            manifest.contains(message_type),
            "Rust host emits \"{message_type}\" but the manifest doesn't list \
            it as hostToBackendToParticipant."
        );
    }

    for manifest_type in &manifest {
        assert!(
            HOST_FORWARDED_OUTBOUND_TYPES.contains(manifest_type),
            "session-relay manifest lists hostToBackendToParticipant={manifest_type}, \
            but Rust does not declare it as a host-emitted forwarded type. \
            Add the corresponding encrypted producer and update \
            HOST_FORWARDED_OUTBOUND_TYPES."
        );
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomainTagAuthority {
    version: u32,
    tags: BTreeMap<String, String>,
    device_proof_preimage_vectors: DeviceProofPreimageVectors,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceProofPreimageVectors {
    connection_v1: ConnectionProofVector,
    http_v1: HttpProofVector,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionProofVector {
    user_id: String,
    device_id: String,
    connection_id: String,
    purpose: String,
    session_id: String,
    incarnation_id: String,
    challenge_hex: String,
    preimage_hex: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpProofVector {
    user_id: String,
    device_id: String,
    challenge_id: String,
    method: String,
    path_and_query: String,
    body_sha256: String,
    challenge_hex: String,
    preimage_hex: String,
}

fn load_domain_tag_authority() -> DomainTagAuthority {
    let path = protocol_dir().join("crypto-domain-tags.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("crypto-domain-tags.json must deserialize")
}

#[test]
fn rust_domain_tags_match_protocol_authority() {
    let authority = load_domain_tag_authority();
    assert_eq!(authority.version, 7);

    let expected: [(&str, &[u8]); 16] = [
        ("DEVICE_POP_V1", kodosi_domain::domain_tags::DEVICE_POP_V1),
        ("DEVICE_CERT_V2", kodosi_domain::domain_tags::DEVICE_CERT_V2),
        ("DEVICE_LIST_V1", kodosi_domain::domain_tags::DEVICE_LIST_V1),
        (
            "DEVICE_CONNECTION_PROOF_V1",
            kodosi_domain::domain_tags::DEVICE_CONNECTION_PROOF_V1,
        ),
        (
            "DEVICE_HTTP_REQUEST_PROOF_V1",
            kodosi_domain::domain_tags::DEVICE_HTTP_REQUEST_PROOF_V1,
        ),
        ("SESSION_KEY_V2", kodosi_domain::domain_tags::SESSION_KEY_V2),
        (
            "SESSION_KEY_BLOB_V2",
            kodosi_domain::domain_tags::SESSION_KEY_BLOB_V2,
        ),
        (
            "CONTROL_MESSAGE_V1",
            kodosi_domain::domain_tags::CONTROL_MESSAGE_V1,
        ),
        (
            "SEMANTIC_REQUEST_V1",
            kodosi_domain::domain_tags::SEMANTIC_REQUEST_V1,
        ),
        (
            "SEMANTIC_CANCEL_V1",
            kodosi_domain::domain_tags::SEMANTIC_CANCEL_V1,
        ),
        (
            "SEMANTIC_RECEIPT_V1",
            kodosi_domain::domain_tags::SEMANTIC_RECEIPT_V1,
        ),
        (
            "SEMANTIC_RECEIPT_ACK_V1",
            kodosi_domain::domain_tags::SEMANTIC_RECEIPT_ACK_V1,
        ),
        (
            "ROOM_CONTENT_V2",
            kodosi_domain::domain_tags::ROOM_CONTENT_V2,
        ),
        ("ROOM_ROSTER_V1", kodosi_domain::domain_tags::ROOM_ROSTER_V1),
        (
            "ROOM_INVITATION_PROPOSAL_V1",
            kodosi_domain::domain_tags::ROOM_INVITATION_PROPOSAL_V1,
        ),
        (
            "ROOM_INVITATION_DECISION_V1",
            kodosi_domain::domain_tags::ROOM_INVITATION_DECISION_V1,
        ),
    ];
    for (name, bytes) in expected {
        let manifest_value = authority
            .tags
            .get(name)
            .unwrap_or_else(|| panic!("manifest missing tag {name}"));
        assert_eq!(
            manifest_value.as_bytes(),
            bytes,
            "domain tag {name} drifted between Rust crypto path and `protocol/crypto-domain-tags.json`"
        );
    }
    assert_eq!(
        authority.tags.len(),
        expected.len(),
        "protocol/crypto-domain-tags.json carries tags unknown to this Rust build"
    );
}

fn decode_lower_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "hex vector length must be even");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("hex vectors must use lowercase ASCII"),
            };
            digit(pair[0]) << 4 | digit(pair[1])
        })
        .collect()
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[test]
fn rust_device_proof_preimages_match_protocol_vectors() {
    let authority = load_domain_tag_authority();
    let connection = authority.device_proof_preimage_vectors.connection_v1;
    let connection_challenge = decode_lower_hex(&connection.challenge_hex);
    let connection_incarnation = uuid::Uuid::parse_str(&connection.incarnation_id).unwrap();
    let actual_connection = kodosi_backend_client::crypto::device_connection_proof_preimage(
        &connection.user_id,
        &connection.device_id,
        &connection.connection_id,
        &connection.purpose,
        Some(&connection.session_id),
        Some(&connection_incarnation),
        &connection_challenge,
    )
    .unwrap();
    assert_eq!(
        encode_lower_hex(&actual_connection),
        connection.preimage_hex
    );

    let http = authority.device_proof_preimage_vectors.http_v1;
    let http_challenge = decode_lower_hex(&http.challenge_hex);
    let challenge_id = uuid::Uuid::parse_str(&http.challenge_id).unwrap();
    let actual_http = kodosi_backend_client::crypto::device_http_request_proof_preimage(
        &http.user_id,
        &http.device_id,
        &challenge_id,
        &http.method,
        &http.path_and_query,
        &http.body_sha256,
        &http_challenge,
    )
    .unwrap();
    assert_eq!(encode_lower_hex(&actual_http), http.preimage_hex);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomInputRulesAuthority {
    version: u32,
    length_unit: String,
    task_statuses: Vec<String>,
    limits: RoomInputLimits,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomInputLimits {
    room_name_max_length: usize,
    room_slug_min_length: usize,
    room_slug_max_length: usize,
    room_slug_pattern: String,
    chat_body_max_length: usize,
    chat_recipient_max_count: usize,
    task_title_max_length: usize,
    task_description_max_length: usize,
    task_result_max_length: usize,
}

fn load_room_input_rules() -> RoomInputRulesAuthority {
    let path = protocol_dir().join("room-input-rules.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("room-input-rules.json must deserialize")
}

#[test]
fn rust_room_input_rules_match_protocol_authority() {
    let authority = load_room_input_rules();
    assert_eq!(authority.version, 1);

    assert_eq!(authority.length_unit, protocol_limits::ROOM_LENGTH_UNIT);
    assert_eq!(
        authority.task_statuses,
        protocol_limits::TASK_STATUSES,
        "room task-status vocabulary drifted between rooms.rs and room-input-rules.json"
    );
    assert_eq!(
        authority.limits.room_name_max_length,
        protocol_limits::ROOM_NAME_MAX_LEN
    );
    assert_eq!(
        authority.limits.room_slug_min_length,
        protocol_limits::ROOM_SLUG_MIN_LEN
    );
    assert_eq!(
        authority.limits.room_slug_max_length,
        protocol_limits::ROOM_SLUG_MAX_LEN
    );
    assert_eq!(
        authority.limits.room_slug_pattern,
        protocol_limits::ROOM_SLUG_PATTERN
    );
    assert_eq!(
        authority.limits.chat_body_max_length,
        protocol_limits::CHAT_BODY_MAX_LEN
    );
    assert_eq!(
        authority.limits.chat_recipient_max_count,
        protocol_limits::CHAT_RECIPIENT_MAX_COUNT
    );
    assert_eq!(
        authority.limits.task_title_max_length,
        protocol_limits::TASK_TITLE_MAX_LEN
    );
    assert_eq!(
        authority.limits.task_description_max_length,
        protocol_limits::TASK_DESCRIPTION_MAX_LEN
    );
    assert_eq!(
        authority.limits.task_result_max_length,
        protocol_limits::TASK_RESULT_MAX_LEN
    );
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityWireFormatAuthority {
    version: u32,
    max_field_length: u32,
    max_entries: u32,
    no_expiry_sentinel: u64,
    max_unix_time_milliseconds: u64,
    max_device_certificate_body_length: usize,
    max_signed_device_list_body_length: usize,
    user_id_encoding: String,
    user_id_length: usize,
    device_id_max_utf16_code_units: usize,
    device_label_max_utf16_code_units: usize,
    ml_kem_768_public_key_length: usize,
    ml_dsa_65_public_key_length: usize,
    ml_dsa_65_signature_length: usize,
    device_list_entry_signer_semantics: String,
    device_list_envelope_signer_must_be_active: bool,
}

fn load_identity_wire_format() -> IdentityWireFormatAuthority {
    let path = protocol_dir().join("identity-wire-format.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("identity-wire-format.json must deserialize")
}

#[test]
fn rust_identity_wire_format_matches_protocol_authority() {
    let authority = load_identity_wire_format();
    assert_eq!(authority.version, 3);
    assert_eq!(
        authority.max_field_length,
        protocol_limits::IDENTITY_MAX_FIELD_LEN,
        "MAX_FIELD_LEN drifted"
    );
    assert_eq!(
        authority.max_entries,
        protocol_limits::IDENTITY_MAX_ENTRIES,
        "MAX_ENTRIES drifted"
    );
    assert_eq!(
        authority.no_expiry_sentinel,
        protocol_limits::IDENTITY_NO_EXPIRY_SENTINEL,
        "NO_EXPIRY_SENTINEL drifted"
    );
    assert_eq!(
        authority.max_unix_time_milliseconds,
        protocol_limits::IDENTITY_MAX_UNIX_TIME_MILLISECONDS,
        "MAX_UNIX_TIME_MILLISECONDS drifted"
    );
    assert_eq!(
        authority.max_device_certificate_body_length,
        protocol_limits::IDENTITY_MAX_DEVICE_CERTIFICATE_BODY_LEN,
        "MAX_DEVICE_CERTIFICATE_BODY_LEN drifted"
    );
    assert_eq!(
        authority.max_signed_device_list_body_length,
        protocol_limits::IDENTITY_MAX_SIGNED_DEVICE_LIST_BODY_LEN,
        "MAX_SIGNED_DEVICE_LIST_BODY_LEN drifted"
    );
    assert_eq!(authority.user_id_encoding, "canonicalLowercaseUuid");
    assert_eq!(authority.user_id_length, 36);
    assert_eq!(
        authority.device_id_max_utf16_code_units,
        protocol_limits::IDENTITY_DEVICE_ID_MAX_UTF16_CODE_UNITS,
        "DEVICE_ID_MAX_UTF16_CODE_UNITS drifted"
    );
    assert_eq!(
        authority.device_label_max_utf16_code_units,
        protocol_limits::IDENTITY_DEVICE_LABEL_MAX_UTF16_CODE_UNITS,
        "DEVICE_LABEL_MAX_UTF16_CODE_UNITS drifted"
    );
    assert_eq!(
        authority.ml_kem_768_public_key_length,
        protocol_limits::IDENTITY_ML_KEM_768_PUBLIC_KEY_LEN,
        "ML_KEM_768_PUBLIC_KEY_LEN drifted"
    );
    assert_eq!(
        authority.ml_dsa_65_public_key_length,
        protocol_limits::IDENTITY_ML_DSA_65_PUBLIC_KEY_LEN,
        "ML_DSA_65_PUBLIC_KEY_LEN drifted"
    );
    assert_eq!(
        authority.ml_dsa_65_signature_length,
        protocol_limits::IDENTITY_ML_DSA_65_SIGNATURE_LEN,
        "ML_DSA_65_SIGNATURE_LEN drifted"
    );
    assert_eq!(
        authority.device_list_entry_signer_semantics,
        "certificateProvenanceMayBeHistorical"
    );
    assert!(authority.device_list_envelope_signer_must_be_active);
}

#[test]
fn user_events_authority_lists_expected_types() {
    let authority = load_user_events_authority();
    assert_eq!(authority.unknown_message_policy, "terminalClose");
    assert_eq!(authority.malformed_payload_policy, "terminalClose");
    let outgoing: BTreeSet<&str> = authority
        .outgoing_types
        .iter()
        .map(String::as_str)
        .collect();

    for expected in [
        "discovery.invalidated",
        "user.deviceListChanged",
        "user.identityLifecycleChanged",
        "user.deviceLinkSnapshot",
        "user.deviceLinkRequested",
        "user.deviceLinkResolved",
    ] {
        assert!(
            outgoing.contains(expected),
            "user-events manifest is missing \"{expected}\" — Rust runtime \
            reads it in backend_client/user_events.rs"
        );
    }
}

#[test]
fn rust_discovery_surfaces_match_user_events_authority() {
    let authority = load_user_events_authority();
    let surfaces: BTreeSet<DiscoverySurface> = authority
        .discovery_surfaces
        .iter()
        .map(|surface| {
            serde_json::from_value(serde_json::Value::String(surface.clone()))
                .unwrap_or_else(|error| panic!("invalid discovery surface {surface}: {error}"))
        })
        .collect();
    let expected = DiscoverySurface::all().into_iter().collect();
    assert_eq!(
        surfaces, expected,
        "DiscoverySurface set drifted between backend_client/user_events.rs \
        and protocol/user-events-authority.json"
    );
}
