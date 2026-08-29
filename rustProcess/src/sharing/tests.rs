use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use aws_lc_rs::{
    kem::{DecapsulationKey, ML_KEM_768},
    signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use time::OffsetDateTime;
use tokio::{net::TcpListener, sync::watch};
use tokio_util::sync::CancellationToken;

use crate::{AppError, config::AppConfig};
use kodosi_backend_client::crypto;
use kodosi_domain::{auth::AuthState, ids::UserId};

use super::scope::SessionKeyDistributionResult;
use kodosi_domain::permissions::{
    AccessLevel, DefaultAudienceAccess, SessionCapabilities, ShareScope,
};

const EVERY_SCOPE: [ShareScope; 4] = [
    ShareScope::JustMe,
    ShareScope::MyDevices,
    ShareScope::Friends,
    ShareScope::Room,
];

fn selected_room() -> super::scope::SelectedRoom {
    super::scope::SelectedRoom {
        id: "01900000-0000-7000-8000-0000000000aa".to_owned(),
        name: "Room".to_owned(),
    }
}

fn default_access_wire_value(request: &impl serde::Serialize) -> String {
    let json = serde_json::to_value(request).expect("backend request should serialize");
    json.get("defaultAccess")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("request should carry a defaultAccess field: {json}"))
        .to_owned()
}

#[test]
fn create_request_never_sends_owner_access_as_the_audience_default() {
    for scope in EVERY_SCOPE {
        let room = (scope == ShareScope::Room).then(selected_room);
        let request = super::backend_adapters::create_session_request(
            "01900000-0000-7000-8000-000000000001".to_owned(),
            uuid::Uuid::now_v7(),
            "session".to_owned(),
            AccessLevel::Inject,
            scope,
            "owner-secret".to_owned(),
            room.as_ref(),
        );
        assert_eq!(
            request.default_access,
            DefaultAudienceAccess::Suggest,
            "{scope:?} must clamp the owner's Inject to an audience default"
        );
        assert!(!request.idempotency_key.is_nil());
        let json = serde_json::to_value(&request).expect("create request should serialize");
        assert!(json.get("idempotencyKey").is_some());
        assert!(json.get("incarnationId").is_none());
        assert_eq!(default_access_wire_value(&request), "Suggest", "{scope:?}");
    }
}

#[test]
fn scope_update_request_never_overwrites_the_existing_audience_default() {
    for scope in EVERY_SCOPE {
        let room = (scope == ShareScope::Room).then(selected_room);
        let room_id = room.as_ref().map(|selected| selected.id.as_str());
        let request = super::backend_adapters::scope_update_request_for_room_id(
            scope,
            room_id,
            uuid::Uuid::from_u128(1),
        );
        assert_eq!(request.default_access, None);
        let json = serde_json::to_value(&request).expect("scope update should serialize");
        assert_eq!(
            json["expectedIncarnationId"],
            "00000000-0000-0000-0000-000000000001"
        );
        assert!(
            json.get("defaultAccess").is_none(),
            "scope-only update must preserve an existing View audience default: {json}"
        );
    }
}

#[test]
fn title_update_request_leaves_the_audience_default_untouched() {
    let request =
        super::backend_adapters::title_update_request("renamed", uuid::Uuid::from_u128(1));
    assert_eq!(request.default_access, None);
    let json = serde_json::to_value(&request).expect("title update should serialize");
    assert_eq!(
        json["expectedIncarnationId"],
        "00000000-0000-0000-0000-000000000001"
    );
    assert!(json.get("defaultAccess").is_none(), "{json}");
}

#[test]
fn backend_devices_are_intersected_with_owner_authorized_users() {
    let allowed = std::collections::HashSet::from(["owner".to_owned()]);
    let filtered = crate::runtime::sharing::filter_authorized_devices(
        "session",
        vec![
            kodosi_backend_client::api::AuthorizedDeviceDto {
                user_id: "owner".to_owned(),
                device_id: "owner-device".to_owned(),
            },
            kodosi_backend_client::api::AuthorizedDeviceDto {
                user_id: "backend-injected".to_owned(),
                device_id: "attacker-device".to_owned(),
            },
        ],
        &allowed,
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].user_id, "owner");
}

#[tokio::test]
async fn local_share_retirement_cancels_relay_that_cloned_delivered_key() {
    let mut app = configured_test_app("http://127.0.0.1:9", "session-rollback", None);
    let session_id = app.state.local.sessions.ids()[0];
    app.state
        .sharing
        .shared_sessions
        .set_session_key(session_id, Some([7; 32]), Some(7));
    let relay_cancellation = CancellationToken::new();
    let (pending_tx, _pending_rx) = watch::channel(None);
    app.state.attach_host_relay(
        session_id,
        relay_cancellation.clone(),
        pending_tx,
        tokio::sync::mpsc::channel(1).0,
        tokio::sync::mpsc::channel(1).0,
        tokio::sync::mpsc::channel(1).0,
        tokio::spawn(std::future::pending::<()>()),
    );
    assert!(app.state.sharing.shared_sessions.get(session_id).is_some());
    app.state.unshare_session_locally(session_id);

    assert!(
        app.state.sharing.shared_sessions.get(session_id).is_none(),
        "a failed share must not leave backend session state behind"
    );
    assert!(!app.state.sharing.host_relays.active(session_id));
    assert!(
        relay_cancellation.is_cancelled(),
        "clearing registry state cannot revoke the key cloned by a running relay"
    );
}

#[tokio::test]
async fn distribute_session_key_returns_pending_when_no_authorized_device_keys_exist() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = configured_test_app(&server.base_url, "session-123", None);

    let outcome = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-123",
        &crypto::generate_session_key().expect("test RNG"),
        7,
    )
    .await
    .unwrap_or_else(|error| panic!("distribution should succeed: {error}"));

    assert_eq!(outcome, SessionKeyDistributionResult::PendingRecipients);
    assert_eq!(server.challenge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.register_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn distribute_session_key_propagates_generic_device_registration_conflict() {
    let server = TestKeyServer::spawn_with_register_problem(
        StatusCode::CONFLICT,
        "CONFLICT",
        "Device already registered.",
        Vec::new(),
        StatusCode::NO_CONTENT,
    )
    .await;
    let mut app = configured_test_app(&server.base_url, "session-456", None);

    let outcome = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-456",
        &crypto::generate_session_key().expect("test RNG"),
        9,
    )
    .await;

    std::assert_matches!(
        outcome,
        Err(AppError::HttpProblem {
            status: 409,
            code: Some(code),
            ..
        }) if code == "CONFLICT"
    );
    assert_eq!(server.challenge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.register_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn distribute_session_key_propagates_device_enrollment_race() {
    let server = TestKeyServer::spawn_with_register_problem(
        StatusCode::CONFLICT,
        "DEVICE_ALREADY_ENROLLED",
        "Device already registered; refresh enrollment challenge and retry.",
        Vec::new(),
        StatusCode::NO_CONTENT,
    )
    .await;
    let mut app = configured_test_app(&server.base_url, "session-457", None);

    let result = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-457",
        &crypto::generate_session_key().expect("test RNG"),
        10,
    )
    .await;

    std::assert_matches!(result, Err(AppError::Unsupported { .. }));
    assert_eq!(server.challenge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.register_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn distribute_session_key_propagates_device_list_generation_race() {
    let server = TestKeyServer::spawn_with_register_problem(
        StatusCode::CONFLICT,
        "DEVICE_LIST_GENERATION_COLLISION",
        "Device list generation collision; refresh identity bundle and retry.",
        Vec::new(),
        StatusCode::NO_CONTENT,
    )
    .await;
    let mut app = configured_test_app(&server.base_url, "session-459", None);

    let result = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-459",
        &crypto::generate_session_key().expect("test RNG"),
        10,
    )
    .await;

    std::assert_matches!(result, Err(AppError::Unsupported { .. }));
    assert_eq!(server.challenge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.register_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn distribute_session_key_propagates_unstructured_device_registration_conflict() {
    let server =
        TestKeyServer::spawn(StatusCode::CONFLICT, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = configured_test_app(&server.base_url, "session-458", None);

    let result = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-458",
        &crypto::generate_session_key().expect("test RNG"),
        10,
    )
    .await;

    std::assert_matches!(
        result,
        Err(AppError::HttpProblem {
            status: 409,
            code: None,
            ..
        })
    );
    assert_eq!(server.challenge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.register_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn distribute_session_key_propagates_store_failures() {
    let viewer_key = DecapsulationKey::generate(&ML_KEM_768)
        .unwrap_or_else(|error| panic!("viewer key generation should succeed: {error:?}"));
    let viewer_public = viewer_key
        .encapsulation_key()
        .unwrap_or_else(|error| panic!("encapsulation key should exist: {error:?}"))
        .key_bytes()
        .unwrap_or_else(|error| panic!("public bytes should be available: {error:?}"))
        .as_ref()
        .to_vec();
    let viewer_user_id = uuid::Uuid::now_v7().to_string();
    let viewer_device_id = "viewer-device".to_owned();
    let viewer_signing = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
        .unwrap_or_else(|error| panic!("viewer signing key generation should succeed: {error:?}"));
    let signed_cert = crate::identity_core::device_cert::build_self_cert(
        &viewer_user_id,
        &viewer_device_id,
        "Viewer device",
        &viewer_public,
        &viewer_signing,
        1_700_000_000_000,
        None,
    )
    .expect("viewer certificate");
    let signed_list = crate::identity_core::signed_device_list::build_bootstrap_list(
        &viewer_user_id,
        &viewer_device_id,
        &viewer_signing,
        1_700_000_000_000,
        None,
    )
    .expect("viewer device list");
    let viewer_signing_public = viewer_signing.public_key().as_ref().to_vec();

    let server = TestKeyServer::spawn(
        StatusCode::CREATED,
        vec![TestAuthorizedDevice {
            user_id: viewer_user_id.clone(),
            device_id: viewer_device_id.clone(),
            identity_bundle: serde_json::json!({
                "userId": viewer_user_id,
                "identityRevision": 1,
                "identityIncarnationId": "01900000-0000-7000-8000-000000000001",
                "deviceList": {
                    "generation": 1,
                    "signerDeviceId": viewer_device_id,
                    "issuedAtMs": 1_700_000_000_000_u64,
                    "expiresAtMs": null,
                    "body": BASE64.encode(&signed_list.body_bytes),
                    "signature": BASE64.encode(&signed_list.signature),
                },
                "devices": [{
                    "deviceId": "viewer-device",
                    "deviceLabel": "Viewer device",
                    "kemPublicKey": BASE64.encode(&viewer_public),
                    "signingPublicKey": BASE64.encode(&viewer_signing_public),
                    "certificate": BASE64.encode(&signed_cert.body_bytes),
                    "certificateSignature": BASE64.encode(&signed_cert.signature),
                    "certSignerDeviceId": "viewer-device",
                    "certIssuedAtMs": 1_700_000_000_000_u64,
                    "certExpiresAtMs": null,
                }],
            }),
        }],
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await;
    let mut app = configured_test_app(
        &server.base_url,
        "session-789",
        Some(viewer_user_id.as_str()),
    );
    app.pin_store
        .bind_to_user("11111111-1111-1111-1111-111111111111")
        .await
        .expect("test pin store should bind to the owner account");

    let result = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-789",
        &crypto::generate_session_key().expect("test RNG"),
        11,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(server.challenge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.register_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_initial_friends_share_is_refused_before_any_key_leaves_the_device() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-friends",
        (ShareScope::Friends, None),
    );

    let error = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-friends",
        &crypto::generate_session_key().expect("test RNG"),
        1,
    )
    .await
    .expect_err("friends scope has no owner-signed audience manifest yet");

    std::assert_matches!(
        error,
        AppError::Unsupported { ref reason } if reason.contains("audience manifest")
    );
    assert_eq!(
        server.authorized_device_calls.load(Ordering::SeqCst),
        0,
        "the guard must fail closed before the backend is asked for recipients"
    );
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_initial_room_share_resolves_the_target_rooms_audience() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-room",
        (ShareScope::Room, Some(selected_room())),
    );

    let error = crate::runtime::sharing::distribute_session_key(
        &mut app,
        "session-room",
        &crypto::generate_session_key().expect("test RNG"),
        1,
    )
    .await
    .expect_err("the selected room is not on this backend");

    std::assert_matches!(error, AppError::NotFound);
    assert_eq!(
        server.room_calls.load(Ordering::SeqCst),
        1,
        "the target room audience must be resolved even though the local record still says just-me"
    );
    assert_eq!(server.store_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_my_devices_share_authorizes_the_owner_and_explicit_grantees_only() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-devices",
        (ShareScope::MyDevices, None),
    );
    let id = app.state.local.sessions.ids()[0];
    app.state
        .sharing
        .shared_sessions
        .get_mut(id)
        .expect("shared session")
        .grant_user_at(
            "guest-user".to_owned(),
            AccessLevel::View,
            OffsetDateTime::now_utc() + time::Duration::hours(1),
        );

    let recipients = crate::runtime::sharing::owner_authorized_recipients(&app, "session-devices")
        .await
        .expect("my-devices recipients resolve without a room");

    assert_eq!(
        recipients
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from([
            "11111111-1111-1111-1111-111111111111".to_owned(),
            "guest-user".to_owned(),
        ])
    );
    assert_eq!(server.room_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn expired_explicit_grant_is_not_authorized_for_key_distribution() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-expired-grant",
        (ShareScope::MyDevices, None),
    );
    let id = app.state.local.sessions.ids()[0];
    app.state
        .sharing
        .shared_sessions
        .get_mut(id)
        .expect("shared session")
        .grant_user_at(
            "expired-user".to_owned(),
            AccessLevel::Inject,
            OffsetDateTime::now_utc() - time::Duration::seconds(1),
        );

    let recipients =
        crate::runtime::sharing::owner_authorized_recipients(&app, "session-expired-grant")
            .await
            .expect("recipient authority resolves");

    assert!(!recipients.contains_key("expired-user"));
    assert_eq!(
        recipients.get("11111111-1111-1111-1111-111111111111"),
        Some(&AccessLevel::Approve)
    );
}

#[tokio::test]
async fn explicit_grants_carry_their_owner_chosen_access_level() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-levels",
        (ShareScope::MyDevices, None),
    );
    let id = app.state.local.sessions.ids()[0];
    {
        let shared = app
            .state
            .sharing
            .shared_sessions
            .get_mut(id)
            .expect("shared session");
        let expires_at = OffsetDateTime::now_utc() + time::Duration::hours(1);
        shared.grant_user_at("watcher".to_owned(), AccessLevel::View, expires_at);
        shared.grant_user_at("operator".to_owned(), AccessLevel::Inject, expires_at);
        shared.grant_user_at("on-call".to_owned(), AccessLevel::Approve, expires_at);
    }

    let recipients = crate::runtime::sharing::owner_authorized_recipients(&app, "session-levels")
        .await
        .expect("my-devices recipients resolve without a room");

    assert_eq!(recipients.get("watcher"), Some(&AccessLevel::View));
    assert_eq!(recipients.get("operator"), Some(&AccessLevel::Inject));
    assert_eq!(recipients.get("on-call"), Some(&AccessLevel::Approve));
    assert_eq!(
        recipients.get("11111111-1111-1111-1111-111111111111"),
        Some(&AccessLevel::Approve),
        "the owner's own devices resolve to the owner mask",
    );

    let watcher = SessionCapabilities::from_access(
        *recipients
            .get("watcher")
            .expect("watcher is in the audience"),
        false,
    );
    assert!(watcher.allows(SessionCapabilities::VIEW));
    assert!(!watcher.allows(SessionCapabilities::SEND_INPUT));

    let owner = SessionCapabilities::from_access(
        *recipients
            .get("11111111-1111-1111-1111-111111111111")
            .expect("owner is in the audience"),
        true,
    );
    assert!(owner.allows(SessionCapabilities::SEND_INPUT));
}

#[tokio::test]
async fn membership_only_grants_default_to_the_audience_floor() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-floor",
        (ShareScope::MyDevices, None),
    );
    let id = app.state.local.sessions.ids()[0];
    app.state
        .sharing
        .shared_sessions
        .get_mut(id)
        .expect("shared session")
        .grant_user_at(
            "guest-user".to_owned(),
            AccessLevel::View,
            OffsetDateTime::now_utc() + time::Duration::hours(1),
        );

    let recipients = crate::runtime::sharing::owner_authorized_recipients(&app, "session-floor")
        .await
        .expect("my-devices recipients resolve without a room");

    assert_eq!(recipients.get("guest-user"), Some(&AccessLevel::View));
    assert!(
        !SessionCapabilities::from_access(AccessLevel::View, false)
            .allows(SessionCapabilities::SEND_INPUT)
    );
}

#[tokio::test]
async fn re_sharing_after_an_unshare_does_not_resurrect_the_previous_audience() {
    let server =
        TestKeyServer::spawn(StatusCode::CREATED, Vec::new(), StatusCode::NO_CONTENT).await;
    let mut app = initial_share_test_app(
        &server.base_url,
        "session-cycle",
        (ShareScope::Room, Some(selected_room())),
    );
    let id = app.state.local.sessions.ids()[0];

    app.state.unshare_session_locally(id);
    assert!(app.state.sharing.shared_sessions.get(id).is_none());

    app.state.sharing.shared_sessions.insert(
        id,
        super::shared_session_registry::SharedSessionState::new(
            "session-cycle-2".to_owned(),
            uuid::Uuid::from_u128(2),
            "owner-secret".to_owned(),
            ShareScope::MyDevices,
            None,
            None,
            None,
        ),
    );

    let shared = app
        .state
        .sharing
        .shared_sessions
        .get(id)
        .expect("re-shared session");
    assert_eq!(shared.scope(), ShareScope::MyDevices);
    assert!(
        shared.room().is_none(),
        "the old room must not survive a re-share"
    );

    let recipients = crate::runtime::sharing::owner_authorized_recipients(&app, "session-cycle-2")
        .await
        .expect("re-shared audience resolves");
    assert_eq!(
        recipients
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from(["11111111-1111-1111-1111-111111111111".to_owned()])
    );
    assert_eq!(
        server.room_calls.load(Ordering::SeqCst),
        0,
        "a my-devices re-share must not consult the room it left"
    );
}

#[derive(Clone)]
struct TestKeyServer {
    base_url: String,
    challenge_calls: Arc<AtomicUsize>,
    register_calls: Arc<AtomicUsize>,
    store_calls: Arc<AtomicUsize>,
    authorized_device_calls: Arc<AtomicUsize>,
    room_calls: Arc<AtomicUsize>,
    _task: Arc<tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
struct TestKeyServerState {
    register_response: TestRegisterDeviceResponse,
    authorized_devices: Vec<TestAuthorizedDevice>,
    store_status: StatusCode,
    challenge_calls: Arc<AtomicUsize>,
    register_calls: Arc<AtomicUsize>,
    store_calls: Arc<AtomicUsize>,
    authorized_device_calls: Arc<AtomicUsize>,
    room_calls: Arc<AtomicUsize>,
}

#[derive(Clone)]
enum TestRegisterDeviceResponse {
    Status(StatusCode),
    Problem {
        status: StatusCode,
        code: &'static str,
        detail: &'static str,
    },
}

#[derive(Clone)]
struct TestAuthorizedDevice {
    user_id: String,
    device_id: String,
    identity_bundle: serde_json::Value,
}

impl TestKeyServer {
    async fn spawn(
        register_status: StatusCode,
        authorized_devices: Vec<TestAuthorizedDevice>,
        store_status: StatusCode,
    ) -> Self {
        Self::spawn_with_register_response(
            TestRegisterDeviceResponse::Status(register_status),
            authorized_devices,
            store_status,
        )
        .await
    }

    async fn spawn_with_register_problem(
        status: StatusCode,
        code: &'static str,
        detail: &'static str,
        authorized_devices: Vec<TestAuthorizedDevice>,
        store_status: StatusCode,
    ) -> Self {
        Self::spawn_with_register_response(
            TestRegisterDeviceResponse::Problem {
                status,
                code,
                detail,
            },
            authorized_devices,
            store_status,
        )
        .await
    }

    async fn spawn_with_register_response(
        register_response: TestRegisterDeviceResponse,
        authorized_devices: Vec<TestAuthorizedDevice>,
        store_status: StatusCode,
    ) -> Self {
        let challenge_calls = Arc::new(AtomicUsize::new(0));
        let register_calls = Arc::new(AtomicUsize::new(0));
        let store_calls = Arc::new(AtomicUsize::new(0));
        let authorized_device_calls = Arc::new(AtomicUsize::new(0));
        let room_calls = Arc::new(AtomicUsize::new(0));
        let state = TestKeyServerState {
            register_response,
            authorized_devices,
            store_status,
            challenge_calls: Arc::clone(&challenge_calls),
            register_calls: Arc::clone(&register_calls),
            store_calls: Arc::clone(&store_calls),
            authorized_device_calls: Arc::clone(&authorized_device_calls),
            room_calls: Arc::clone(&room_calls),
        };

        let router = Router::new()
            .route(
                "/api/me/devices/challenge",
                post(issue_registration_challenge),
            )
            .route("/api/me/devices", post(register_device))
            .route(
                "/api/sessions/{session_id}/keys/authorized-devices",
                get(list_authorized_devices),
            )
            .route("/api/users/{user_id}/identity", get(fetch_identity_bundle))
            .route("/api/sessions/{session_id}/keys", post(store_key_blobs))
            .route("/api/rooms/", get(list_rooms))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| panic!("test listener should bind: {error}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("listener address should exist: {error}"));
        let task = Arc::new(tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .unwrap_or_else(|error| panic!("test key server should run: {error}"));
        }));

        Self {
            base_url: format!("http://{address}"),
            challenge_calls,
            register_calls,
            store_calls,
            authorized_device_calls,
            room_calls,
            _task: task,
        }
    }
}

async fn register_device(State(state): State<TestKeyServerState>) -> Response {
    state.register_calls.fetch_add(1, Ordering::SeqCst);
    match state.register_response {
        TestRegisterDeviceResponse::Status(status) => status.into_response(),
        TestRegisterDeviceResponse::Problem {
            status,
            code,
            detail,
        } => (
            status,
            Json(serde_json::json!({
                "type": format!("https://kodosi.dev/errors/{}", code.to_ascii_lowercase()),
                "title": "Conflict",
                "status": status.as_u16(),
                "detail": detail,
                "code": code,
            })),
        )
            .into_response(),
    }
}

async fn issue_registration_challenge(
    State(state): State<TestKeyServerState>,
) -> Json<serde_json::Value> {
    state.challenge_calls.fetch_add(1, Ordering::SeqCst);
    Json(serde_json::json!({
        "challengeId": "00000000-0000-0000-0000-000000000000",
        "challengeBytes": BASE64.encode([0u8; 32]),
        "expiresAt": "2099-01-01T00:00:00Z",
    }))
}

async fn list_authorized_devices(
    State(state): State<TestKeyServerState>,
) -> Json<Vec<serde_json::Value>> {
    state.authorized_device_calls.fetch_add(1, Ordering::SeqCst);
    Json(
        state
            .authorized_devices
            .into_iter()
            .map(|device| {
                serde_json::json!({
                    "userId": device.user_id,
                    "deviceId": device.device_id,
                })
            })
            .collect(),
    )
}

async fn list_rooms(State(state): State<TestKeyServerState>) -> Response {
    state.room_calls.fetch_add(1, Ordering::SeqCst);
    let mut response = Json(Vec::<serde_json::Value>::new()).into_response();
    response.headers_mut().insert(
        "kodosi-has-more",
        axum::http::HeaderValue::from_static("false"),
    );
    response
}

async fn fetch_identity_bundle(
    State(state): State<TestKeyServerState>,
    Path(user_id): Path<String>,
) -> Response {
    state
        .authorized_devices
        .iter()
        .find(|device| device.user_id == user_id)
        .map_or_else(
            || StatusCode::NOT_FOUND.into_response(),
            |device| Json(device.identity_bundle.clone()).into_response(),
        )
}

async fn store_key_blobs(State(state): State<TestKeyServerState>) -> StatusCode {
    state.store_calls.fetch_add(1, Ordering::SeqCst);
    state.store_status
}

fn configured_test_app(
    base_url: &str,
    backend_session_id: &str,
    explicit_grantee: Option<&str>,
) -> crate::runtime::Runtime {
    configured_test_app_sharing(base_url, backend_session_id, explicit_grantee, None)
}

fn initial_share_test_app(
    base_url: &str,
    backend_session_id: &str,
    target: (ShareScope, Option<super::scope::SelectedRoom>),
) -> crate::runtime::Runtime {
    let app = configured_test_app_sharing(base_url, backend_session_id, None, Some(target));
    let id = app.state.local.sessions.ids()[0];
    assert_eq!(
        app.state
            .local
            .sessions
            .record(id)
            .map(|record| record.summary.scope),
        Some(ShareScope::JustMe),
        "an initial share must be exercised with the pre-commit local record"
    );
    app
}

fn configured_test_app_sharing(
    base_url: &str,
    backend_session_id: &str,
    explicit_grantee: Option<&str>,
    initial_share: Option<(ShareScope, Option<super::scope::SelectedRoom>)>,
) -> crate::runtime::Runtime {
    let mut config = AppConfig::default();
    config.auth.keyring_service = format!("kodosi.test.{}", uuid::Uuid::now_v7());
    config.backend.api = Some(base_url.to_owned());
    let mut app = crate::runtime::Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        crate::runtime::RuntimeDependencies::isolated(&config)
            .expect("isolated runtime dependencies"),
    )
    .unwrap_or_else(|error| panic!("test app should construct: {error}"));
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(
            UserId::try_from("11111111-1111-1111-1111-111111111111")
                .unwrap_or_else(|error| panic!("test user id should parse: {error}")),
        ),
        expires_at: OffsetDateTime::now_utc(),
    };
    let id = kodosi_domain::ids::SessionId::new();
    let mut summary = kodosi_domain::session::SessionSummary::new_owned(
        id,
        "test".to_owned(),
        "test-runtime".to_owned(),
        kodosi_domain::terminal::TerminalSize::default(),
        app.state.identity.auth.subject(),
    );
    let (recorded_scope, recorded_room) = match &initial_share {
        Some((scope, room)) => (
            *scope,
            room.as_ref()
                .map(super::shared_session_registry::SharedRoom::from),
        ),
        None => (ShareScope::MyDevices, None),
    };
    summary.scope = if initial_share.is_some() {
        ShareScope::JustMe
    } else {
        ShareScope::MyDevices
    };
    app.state.local.sessions.insert(summary);
    let mut shared = super::shared_session_registry::SharedSessionState::new(
        backend_session_id.to_owned(),
        uuid::Uuid::from_u128(1),
        "owner-secret".to_owned(),
        recorded_scope,
        recorded_room,
        None,
        None,
    );
    if let Some(grantee) = explicit_grantee {
        shared.grant_user_at(
            grantee.to_owned(),
            AccessLevel::View,
            OffsetDateTime::now_utc() + time::Duration::hours(1),
        );
    }
    app.state.sharing.shared_sessions.insert(id, shared);
    app
}
