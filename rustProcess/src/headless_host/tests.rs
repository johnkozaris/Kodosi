use super::{
    HeadlessHostAuthState,
    client::{session_error_matches_identifier, session_matches_identifier},
    dispatch::{apply_active_session_event, dispatch_runtime_events},
    server::HeadlessHostActivity,
    snapshot::{HeadlessHostState, apply_snapshot_message},
};
use crate::{
    AuthEvent, DeviceEvent, FriendsEvent, HostEvent, RuntimeSessionStatus, SessionEvent,
    SessionListEntry, TrustEvent,
    host_protocol::{
        LocalSessionListEntry, PermissionFlags, RemoteSessionListEntry, RoomListEntry,
        SessionSemanticActions, room_catalog_event, session_catalog_snapshot_event,
    },
    runtime_event_bus::runtime_event_channels,
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessState},
    permissions::{AccessLevel, ShareScope},
    session::SessionMode,
};
use std::collections::BTreeSet;
use tokio::{sync::broadcast, time};
use tokio_util::sync::CancellationToken;

fn sample_local_session(status: RuntimeSessionStatus) -> SessionListEntry {
    sample_local_session_with_id("session-1", status, "Session")
}

fn sample_local_session_with_id(
    id: &str,
    status: RuntimeSessionStatus,
    name: &str,
) -> SessionListEntry {
    SessionListEntry::Local {
        entry: LocalSessionListEntry {
            id: id.to_owned(),
            incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            create_request_id: Some(if id == "session-1" {
                "req-1".to_owned()
            } else {
                format!("create-{id}")
            }),
            name: name.to_owned(),
            project: "local".to_owned(),
            mode: SessionMode::Normal,
            status,
            recovery: kodosi_domain::session::LocalSessionRecoveryState::Live,
            scope: ShareScope::JustMe,
            access: AccessLevel::Inject,
            room_id: None,
            room_name: None,
            active_count: 0,
            entitled_count: 1,
            last_activity: "just now".to_owned().into(),
            semantic_actions: SessionSemanticActions::default(),
            backend_session_id: None,
            backend_incarnation_id: None,
            meta: None,
        },
    }
}

fn sample_owned_remote_session() -> SessionListEntry {
    SessionListEntry::Remote {
        entry: RemoteSessionListEntry {
            id: "session-1".to_owned(),
            incarnation_id: Some("01900000-0000-7000-8000-000000000002".to_owned()),
            name: "Session".to_owned(),
            project: "local".to_owned(),
            mode: SessionMode::Normal,
            status: RuntimeSessionStatus::Active,
            scope: ShareScope::Friends,
            access: AccessLevel::Inject,
            owner: None,
            owner_user_id: Some("owner-1".to_owned()),
            permissions: PermissionFlags::OWNER,
            room_id: None,
            room_name: None,
            connection_state: Some(ConnectionState::Connected),
            connection_reason: None,
            access_state: Some(RemoteSessionAccessState::Ready),
            access_reason: None,
            access_issue: None,
            active_count: 1,
            entitled_count: 5,
            last_activity: "just now".to_owned().into(),
            semantic_actions: SessionSemanticActions::default(),
        },
    }
}

fn friends_error() -> FriendsEvent {
    FriendsEvent::Error {
        operation: "request.send".to_owned(),
        message: "not found".to_owned(),
        request_id: None,
    }
}

fn devices_error() -> DeviceEvent {
    DeviceEvent::Error {
        user_code: None,
        operation: "refresh".to_owned(),
        message: "backend unavailable".to_owned(),
    }
}

fn room(id: &str, name: &str) -> RoomListEntry {
    RoomListEntry {
        id: id.to_owned(),
        name: name.to_owned(),
        slug: name.to_ascii_lowercase(),
    }
}

#[test]
fn active_local_sessions_keep_the_host_alive() {
    assert!(sample_local_session(RuntimeSessionStatus::Active).is_active_local());
    assert!(!sample_local_session(RuntimeSessionStatus::Stopped).is_active_local());
}

#[test]
fn active_session_watch_tracks_local_delta_events() {
    let mut active_local_sessions = BTreeSet::new();
    let mut active_remote_relays = BTreeSet::new();

    assert!(apply_active_session_event(
        &mut active_local_sessions,
        &mut active_remote_relays,
        &SessionEvent::List {
            sessions: vec![sample_local_session(RuntimeSessionStatus::Active)],
        },
    ));
    assert_eq!(
        active_local_sessions,
        BTreeSet::from(["session-1".to_owned()])
    );

    assert!(apply_active_session_event(
        &mut active_local_sessions,
        &mut active_remote_relays,
        &SessionEvent::Upsert {
            session: Box::new(sample_local_session(RuntimeSessionStatus::Stopped)),
        },
    ));
    assert!(active_local_sessions.is_empty());

    assert!(apply_active_session_event(
        &mut active_local_sessions,
        &mut active_remote_relays,
        &SessionEvent::Upsert {
            session: Box::new(sample_local_session(RuntimeSessionStatus::Active)),
        },
    ));
    assert_eq!(
        active_local_sessions,
        BTreeSet::from(["session-1".to_owned()])
    );

    assert!(apply_active_session_event(
        &mut active_local_sessions,
        &mut active_remote_relays,
        &SessionEvent::Removed {
            session_id: "session-1".to_owned(),
        },
    ));
    assert!(active_local_sessions.is_empty());
}

#[test]
fn active_session_watch_keeps_local_state_across_remote_upserts() {
    let mut active_local_sessions = BTreeSet::new();
    let mut active_remote_relays = BTreeSet::new();

    assert!(apply_active_session_event(
        &mut active_local_sessions,
        &mut active_remote_relays,
        &SessionEvent::List {
            sessions: vec![
                sample_local_session(RuntimeSessionStatus::Active),
                sample_owned_remote_session(),
            ],
        },
    ));
    assert_eq!(
        active_local_sessions,
        BTreeSet::from(["session-1".to_owned()])
    );
    assert_eq!(
        active_remote_relays,
        BTreeSet::from(["session-1".to_owned()])
    );

    assert!(apply_active_session_event(
        &mut active_local_sessions,
        &mut active_remote_relays,
        &SessionEvent::Upsert {
            session: Box::new(sample_owned_remote_session()),
        },
    ));
    assert!(active_local_sessions.contains("session-1"));
}

#[test]
fn remote_relay_and_other_pending_work_keep_headless_host_alive() {
    let mut local = BTreeSet::new();
    let mut remote = BTreeSet::new();
    assert!(apply_active_session_event(
        &mut local,
        &mut remote,
        &SessionEvent::Upsert {
            session: Box::new(sample_owned_remote_session()),
        },
    ));
    assert_eq!(remote, BTreeSet::from(["session-1".to_owned()]));

    for activity in [
        HeadlessHostActivity {
            active_remote_relays: 1,
            ..HeadlessHostActivity::default()
        },
        HeadlessHostActivity {
            pending_auth: true,
            ..HeadlessHostActivity::default()
        },
        HeadlessHostActivity {
            pending_device_link: true,
            ..HeadlessHostActivity::default()
        },
        HeadlessHostActivity {
            connected_clients: 1,
            ..HeadlessHostActivity::default()
        },
    ] {
        assert!(activity.keeps_host_alive());
    }
    assert!(!HeadlessHostActivity::default().keeps_host_alive());
}

#[tokio::test]
async fn dispatch_runtime_events_broadcasts_friends_lane() {
    let (runtime_tx, runtime_rx) = runtime_event_channels();
    let (events_tx, mut events_rx) = broadcast::channel(8);
    let (activity_tx, _activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());
    let shutdown = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_rx,
        events_tx,
        activity_tx,
        shutdown.clone(),
        CancellationToken::new(),
    ));

    time::timeout(
        std::time::Duration::from_secs(1),
        runtime_tx.send_friends(Some("user-1".to_owned()), 1, friends_error()),
    )
    .await
    .unwrap_or_else(|_| panic!("friends lane send should not block"))
    .unwrap_or_else(|error| panic!("friends lane should send: {error}"));
    std::assert_matches!(
        events_rx
            .recv()
            .await
            .unwrap_or_else(|error| panic!("friends event should broadcast: {error}")),
        HostEvent::Friends(FriendsEvent::Error { .. })
    );

    drop(runtime_tx);
    dispatch_handle
        .await
        .unwrap_or_else(|error| panic!("dispatch task should join: {error}"));
    assert!(shutdown.is_cancelled());
}

#[tokio::test]
async fn dispatch_runtime_events_broadcasts_devices_lane() {
    let (runtime_tx, runtime_rx) = runtime_event_channels();
    let (events_tx, mut events_rx) = broadcast::channel(8);
    let (activity_tx, _activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());
    let shutdown = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_rx,
        events_tx,
        activity_tx,
        shutdown.clone(),
        CancellationToken::new(),
    ));

    time::timeout(
        std::time::Duration::from_secs(1),
        runtime_tx.send_devices(Some("user-1".to_owned()), 1, devices_error()),
    )
    .await
    .unwrap_or_else(|_| panic!("devices lane send should not block"))
    .unwrap_or_else(|error| panic!("devices lane should send: {error}"));
    std::assert_matches!(
        events_rx
            .recv()
            .await
            .unwrap_or_else(|error| panic!("devices event should broadcast: {error}")),
        HostEvent::Devices(DeviceEvent::Error { .. })
    );

    drop(runtime_tx);
    dispatch_handle
        .await
        .unwrap_or_else(|error| panic!("dispatch task should join: {error}"));
    assert!(shutdown.is_cancelled());
}

#[tokio::test]
async fn dispatch_runtime_events_broadcasts_trust_lane() {
    let (runtime_tx, runtime_rx) = runtime_event_channels();
    let (events_tx, mut events_rx) = broadcast::channel(8);
    let (activity_tx, _activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());
    let shutdown = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_rx,
        events_tx,
        activity_tx,
        shutdown.clone(),
        CancellationToken::new(),
    ));

    runtime_tx
        .send_trust(
            Some("user-1".to_owned()),
            1,
            TrustEvent::Reset {
                request_id: "request-1".to_owned(),
                user_id: "user-1".to_owned(),
                cleared: true,
            },
        )
        .await
        .unwrap_or_else(|error| panic!("trust lane should send: {error}"));
    std::assert_matches!(
        events_rx
            .recv()
            .await
            .unwrap_or_else(|error| panic!("trust event should broadcast: {error}")),
        HostEvent::Trust(TrustEvent::Reset {
            request_id,
            user_id,
            cleared: true,
        }) if request_id == "request-1" && user_id == "user-1"
    );

    drop(runtime_tx);
    dispatch_handle
        .await
        .unwrap_or_else(|error| panic!("dispatch task should join: {error}"));
    assert!(shutdown.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_dispatcher_still_delivers_the_runtimes_shutdown_events() {
    let (runtime_tx, runtime_rx) = runtime_event_channels();
    let (events_tx, mut events_rx) = broadcast::channel(8);
    let (activity_tx, _activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());
    let shutdown = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_rx,
        events_tx,
        activity_tx,
        shutdown.clone(),
        CancellationToken::new(),
    ));

    shutdown.cancel();

    time::timeout(
        std::time::Duration::from_secs(1),
        runtime_tx.send_session(
            None,
            1,
            SessionEvent::Removed {
                session_id: "session-1".to_owned(),
            },
        ),
    )
    .await
    .unwrap_or_else(|_| panic!("a shutdown publish must not block"))
    .unwrap_or_else(|error| panic!("the lane must still accept it: {error}"));

    std::assert_matches!(
        time::timeout(std::time::Duration::from_secs(1), events_rx.recv())
            .await
            .unwrap_or_else(|_| panic!("the event must still be broadcast"))
            .unwrap_or_else(|error| panic!("broadcast should deliver: {error}")),
        HostEvent::Session(SessionEvent::Removed { .. })
    );

    drop(runtime_tx);
    dispatch_handle
        .await
        .unwrap_or_else(|error| panic!("dispatch task should join: {error}"));
}

#[tokio::test(start_paused = true)]
async fn a_dispatcher_leaves_as_soon_as_the_runtime_reports_it_has_finished() {
    let (runtime_tx, runtime_rx) = runtime_event_channels();
    let (events_tx, mut events_rx) = broadcast::channel(8);
    let (activity_tx, _activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());
    let shutdown = CancellationToken::new();
    let runtime_finished = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_rx,
        events_tx,
        activity_tx,
        shutdown.clone(),
        runtime_finished.clone(),
    ));

    shutdown.cancel();
    runtime_tx
        .send_session(
            None,
            1,
            SessionEvent::Removed {
                session_id: "session-1".to_owned(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("the lane must still accept it: {error}"));
    runtime_finished.cancel();

    let started = time::Instant::now();

    let host_side_sender = runtime_tx;
    time::timeout(crate::shutdown::HOST_SHUTDOWN_BUDGET, dispatch_handle)
        .await
        .unwrap_or_else(|_| panic!("the dispatcher must leave with the runtime"))
        .unwrap_or_else(|error| panic!("dispatch task should join: {error}"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "leaving must follow the runtime, not the window"
    );
    std::assert_matches!(
        events_rx
            .try_recv()
            .unwrap_or_else(|error| panic!("the buffered tail must still be published: {error}")),
        HostEvent::Session(SessionEvent::Removed { .. })
    );
    drop(host_side_sender);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_dispatcher_leaves_when_its_drain_window_closes() {
    let (runtime_tx, runtime_rx) = runtime_event_channels();
    let (events_tx, _events_rx) = broadcast::channel(8);
    let (activity_tx, _activity_rx) = tokio::sync::watch::channel(HeadlessHostActivity::default());
    let shutdown = CancellationToken::new();
    let dispatch_handle = tokio::spawn(dispatch_runtime_events(
        runtime_rx,
        events_tx,
        activity_tx,
        shutdown.clone(),
        CancellationToken::new(),
    ));

    shutdown.cancel();

    let host_side_sender = runtime_tx;
    time::timeout(
        crate::shutdown::HOST_SHUTDOWN_BUDGET + std::time::Duration::from_secs(5),
        dispatch_handle,
    )
    .await
    .unwrap_or_else(|_| panic!("the dispatcher must not outlive its drain window"))
    .unwrap_or_else(|error| panic!("dispatch task should join: {error}"));
    drop(host_side_sender);
}

#[test]
fn session_identifier_matches_session_id_and_create_request_id() {
    let session = sample_local_session(RuntimeSessionStatus::Active);
    assert!(session_matches_identifier(&session, "session-1"));
    assert!(session_matches_identifier(&session, "req-1"));
    assert!(!session_matches_identifier(&session, "other"));
}

#[test]
fn waiters_only_fail_for_correlated_session_errors() {
    assert!(session_error_matches_identifier(
        Some("session-1"),
        None,
        "session-1"
    ));
    assert!(session_error_matches_identifier(
        None,
        Some("request-1"),
        "request-1"
    ));
    assert!(!session_error_matches_identifier(
        Some("other-session"),
        Some("other-request"),
        "session-1"
    ));
    assert!(!session_error_matches_identifier(None, None, "session-1"));
}

#[test]
fn snapshot_is_incomplete_without_auth_state() {
    let snapshot = HeadlessHostState::default();
    std::assert_matches!(snapshot.auth, HeadlessHostAuthState::Unknown);
}

#[test]
fn snapshot_ignores_auth_notices() {
    let mut snapshot = HeadlessHostState::default();

    apply_snapshot_message(
        &mut snapshot,
        HostEvent::Auth(AuthEvent::Notice { message: None }),
    )
    .unwrap_or_else(|error| panic!("auth notice should be non-fatal: {error}"));

    std::assert_matches!(snapshot.auth, HeadlessHostAuthState::Unknown);
}

#[test]
fn snapshot_retains_ready_account_identity() {
    let mut snapshot = HeadlessHostState::default();

    apply_snapshot_message(
        &mut snapshot,
        HostEvent::Auth(AuthEvent::Ready {
            user_id: Some("11111111-1111-1111-1111-111111111111".to_owned()),
            account_epoch: 1,
        }),
    )
    .expect("valid auth identity should be retained");

    std::assert_matches!(snapshot.auth, HeadlessHostAuthState::Ready);
    assert_eq!(
        snapshot.auth_user_id.map(|user_id| user_id.to_string()),
        Some("11111111-1111-1111-1111-111111111111".to_owned())
    );
}

#[test]
fn snapshot_keeps_its_own_refresh_failure_fatal() {
    let mut snapshot = HeadlessHostState::default();

    let error = apply_snapshot_message(
        &mut snapshot,
        HostEvent::Session(SessionEvent::Error {
            operation: "session.list".to_owned(),
            session_id: None,
            request_id: None,
            message: "backend refused the request".to_owned(),
        }),
    )
    .expect_err("a failure of the snapshot's own refresh must stay fatal");

    assert!(
        error
            .to_string()
            .contains("session.list: backend refused the request")
    );
}

mod snapshot_error_correlation {
    use super::{
        HeadlessHostState, RuntimeSessionStatus, apply_snapshot_message,
        sample_local_session_with_id,
    };
    use crate::{
        AuthEvent, HostEvent, SessionEvent, SystemEvent,
        host_protocol::session_catalog_snapshot_event,
    };

    const OTHER_SESSION: &str = "019d1bd1-c0ae-72a0-88cb-c8519739adaa";

    fn session_error(
        operation: &str,
        session_id: Option<&str>,
        request_id: Option<&str>,
    ) -> HostEvent {
        HostEvent::Session(SessionEvent::Error {
            operation: operation.to_owned(),
            session_id: session_id.map(ToOwned::to_owned),
            request_id: request_id.map(ToOwned::to_owned),
            message: "the runtime refused it".to_owned(),
        })
    }

    #[test]
    fn another_clients_session_failure_does_not_fail_this_snapshot() {
        let mut snapshot = HeadlessHostState::default();

        for event in [
            session_error("session.stop", Some(OTHER_SESSION), None),
            session_error("session.delete", Some(OTHER_SESSION), None),
            session_error("session.create", None, Some("another-clients-request")),
            session_error("session.scope", Some(OTHER_SESSION), Some("req-scope")),
            session_error("session.openRemote", Some(OTHER_SESSION), None),
        ] {
            apply_snapshot_message(&mut snapshot, event).unwrap_or_else(|error| {
                panic!("another client's failure must be ignored: {error}")
            });
        }
    }

    #[test]
    fn the_snapshots_own_refresh_failure_is_still_its_verdict() {
        let mut snapshot = HeadlessHostState::default();

        let error =
            apply_snapshot_message(&mut snapshot, session_error("session.list", None, None))
                .expect_err("the snapshot's own refresh failure must surface");

        assert!(
            error.to_string().contains("session.list"),
            "the verdict must name the operation, got: {error}"
        );
    }

    #[test]
    fn a_targeted_refresh_failure_belongs_to_the_client_that_targeted_it() {
        let mut snapshot = HeadlessHostState::default();

        apply_snapshot_message(
            &mut snapshot,
            session_error("session.list", Some(OTHER_SESSION), None),
        )
        .unwrap_or_else(|error| panic!("a targeted refresh is not this snapshot's: {error}"));
    }

    #[test]
    fn a_runtime_system_error_the_snapshot_did_not_cause_is_ignored() {
        let mut snapshot = HeadlessHostState::default();

        apply_snapshot_message(
            &mut snapshot,
            HostEvent::System(SystemEvent::Error {
                message: "backend refused the request".to_owned(),
                context: Some("session.input:session-1".to_owned()),
            }),
        )
        .unwrap_or_else(|error| panic!("a terminal-lane failure is not the snapshot's: {error}"));
    }

    #[test]
    fn a_snapshot_still_completes_while_another_client_is_failing() {
        let mut snapshot = HeadlessHostState::default();
        let sessions = vec![sample_local_session_with_id(
            "session-1",
            RuntimeSessionStatus::Active,
            "Mine",
        )];

        for event in [
            HostEvent::System(SystemEvent::Error {
                message: "could not parse host command".to_owned(),
                context: Some("{\"type\":\"garbage\"}".to_owned()),
            }),
            session_error("session.stop", Some(OTHER_SESSION), None),
            HostEvent::Session(session_catalog_snapshot_event(sessions.clone())),
            HostEvent::Auth(AuthEvent::Ready {
                user_id: None,
                account_epoch: 1,
            }),
        ] {
            apply_snapshot_message(&mut snapshot, event)
                .unwrap_or_else(|error| panic!("the snapshot must survive the noise: {error}"));
        }

        assert_eq!(snapshot.sessions, sessions);
    }
}

#[cfg(unix)]
mod room_action_client_wire {
    use tokio::sync::oneshot;
    use uuid::Uuid;

    use super::super::{
        client::HeadlessHostClient,
        handshake::{
            ConnectionLane, ControlClientFrame, ControlServerFrame, HOST_PROTOCOL_VERSION,
            HostHelloRequest, HostHelloResponse,
        },
        local_endpoint::{accept_local, bind_local, connect_local, socket_path_for_dir},
    };
    use crate::{
        HostCommand, HostEvent, RoomActionStatus, RoomCommand, RoomEvent, support::io::framed_json,
    };

    const TOKEN: &str = "token";

    #[tokio::test]
    async fn room_action_sends_command_and_ignores_unrelated_result() {
        let dir = tempfile::tempdir().expect("temp dir");
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let request_id = Uuid::now_v7().to_string();
        let expected_request_id = request_id.clone();
        let (seen_tx, seen_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let stream = accept_local(&listener).await.expect("accept client");
            let (read_half, write_half) = stream.into_split();
            let mut reader = framed_json::reader(read_half);
            let mut writer = framed_json::writer(write_half);
            let hello: HostHelloRequest = framed_json::read_json(&mut reader)
                .await
                .expect("read hello")
                .expect("hello frame");
            assert_eq!(hello.token, TOKEN);
            framed_json::write_json(&mut writer, &HostHelloResponse::accept())
                .await
                .expect("write hello");
            let frame: ControlClientFrame = framed_json::read_json(&mut reader)
                .await
                .expect("read command")
                .expect("command frame");
            let ControlClientFrame::Command { command } = frame else {
                panic!("expected command frame")
            };
            let command: HostCommand = serde_json::from_value(command).expect("typed command");
            let HostCommand::Room(RoomCommand::TaskAssign {
                request_id,
                room_id,
                task_id,
                ..
            }) = command
            else {
                panic!("expected task assignment")
            };
            assert_eq!(request_id, expected_request_id);
            assert_eq!(room_id, "room-1");
            assert_eq!(task_id, "task-1");
            let _ = seen_tx.send(());
            for event in [
                HostEvent::Room(RoomEvent::ActionResult {
                    request_id: Uuid::now_v7().to_string(),
                    operation: "tasks.assign".to_owned(),
                    room_id: Some("room-1".to_owned()),
                    fingerprint: Some("other".to_owned()),
                    status: RoomActionStatus::Succeeded,
                    entity_id: Some("task-1".to_owned()),
                    message: None,
                }),
                HostEvent::Room(RoomEvent::ActionAccepted {
                    request_id: request_id.clone(),
                    operation: "tasks.assign".to_owned(),
                    room_id: "room-1".to_owned(),
                    fingerprint: "hash".to_owned(),
                }),
                HostEvent::Room(RoomEvent::ActionResult {
                    request_id,
                    operation: "tasks.assign".to_owned(),
                    room_id: Some("room-1".to_owned()),
                    fingerprint: Some("hash".to_owned()),
                    status: RoomActionStatus::Succeeded,
                    entity_id: Some("task-1".to_owned()),
                    message: None,
                }),
            ] {
                framed_json::write_json(
                    &mut writer,
                    &ControlServerFrame::Event {
                        event: Box::new(event),
                    },
                )
                .await
                .expect("write event");
            }
        });

        let stream = connect_local(&socket).await.expect("connect client");
        let mut framed = framed_json::framed(stream);
        framed_json::send_json(
            &mut framed,
            &HostHelloRequest {
                token: TOKEN.to_owned(),
                protocol_version: HOST_PROTOCOL_VERSION,
                lane: ConnectionLane::Control,
            },
        )
        .await
        .expect("send hello");
        let response: HostHelloResponse = framed_json::next_json(&mut framed)
            .await
            .expect("read hello")
            .expect("hello response");
        assert!(response.accepted);
        let mut client = HeadlessHostClient {
            framed,
            account_user_id: None,
            account_epoch: 1,
            remote_operations_ready: true,
            pending_events: std::collections::VecDeque::new(),
        };
        client
            .send_room_action(RoomCommand::TaskAssign {
                room_id: "room-1".to_owned(),
                task_id: "task-1".to_owned(),
                expected_task_revision: 3,
                session_id: None,
                session_incarnation_id: None,
                request_id,
            })
            .await
            .expect("correlated action succeeds");
        seen_rx.await.expect("server saw command");
        server.await.expect("server task");
    }
}

#[cfg(unix)]
mod concurrent_client_snapshot {
    use std::time::Duration;

    use tokio::{sync::broadcast, time};

    use super::super::{
        client::HeadlessHostClient,
        handshake::{
            ConnectionLane, ControlClientFrame, ControlServerFrame, HOST_PROTOCOL_VERSION,
            HostHelloRequest, HostHelloResponse,
        },
        local_endpoint::{accept_local, bind_local, connect_local, socket_path_for_dir},
    };
    use super::{HeadlessHostAuthState, RuntimeSessionStatus, sample_local_session_with_id};
    use crate::{
        AuthEvent, HostCommand, HostEvent, RemoteCommandStatus, SessionCommand, SessionEvent,
        support::io::framed_json,
    };

    const OTHER_SESSION: &str = "019d1bd1-c0ae-72a0-88cb-c8519739adaa";
    const TOKEN: &str = "token";

    fn other_clients_failure() -> HostEvent {
        HostEvent::Session(SessionEvent::Error {
            operation: "session.stop".to_owned(),
            session_id: Some(OTHER_SESSION.to_owned()),
            request_id: None,
            message: "cannot stop a session this device does not own".to_owned(),
        })
    }

    async fn serve_client(
        stream: super::super::local_endpoint::LocalStream,
        events_tx: broadcast::Sender<HostEvent>,
        hello_ready: bool,
        snapshot_ready: bool,
        event_before_snapshot: bool,
    ) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = framed_json::reader(read_half);
        let mut writer = framed_json::writer(write_half);
        let Ok(Some(hello)) = framed_json::read_json::<_, HostHelloRequest>(&mut reader).await
        else {
            return;
        };
        let accepted = hello.token == TOKEN && hello.protocol_version == HOST_PROTOCOL_VERSION;
        let response = if accepted {
            HostHelloResponse::accept_control(RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                remote_operations_ready: hello_ready,
            })
        } else {
            HostHelloResponse::reject("invalid host token")
        };
        if framed_json::write_json(&mut writer, &response)
            .await
            .is_err()
            || !accepted
        {
            return;
        }

        let mut events_rx = events_tx.subscribe();
        loop {
            tokio::select! {
                frame = framed_json::read_json::<_, ControlClientFrame>(&mut reader) => {
                    let Ok(Some(frame)) = frame else { break };
                    if let ControlClientFrame::Snapshot { refresh_id } = frame {
                        if event_before_snapshot
                            && framed_json::write_json(
                                &mut writer,
                                &ControlServerFrame::Event {
                                    event: Box::new(other_clients_failure()),
                                },
                            )
                            .await
                            .is_err()
                        {
                            break;
                        }
                        let response = ControlServerFrame::Snapshot {
                            refresh_id,
                            account_user_id: None,
                            account_epoch: 1,
                            remote_operations_ready: snapshot_ready,
                            auth: AuthEvent::Ready {
                                user_id: None,
                                account_epoch: 1,
                            },
                            sessions: vec![sample_local_session_with_id(
                                "session-1",
                                RuntimeSessionStatus::Active,
                                "Mine",
                            )],
                            rooms: Vec::new(),
                        };
                        if framed_json::write_json(&mut writer, &response).await.is_err() {
                            break;
                        }
                    }

                }
                event = events_rx.recv() => {
                    let Ok(event) = event else { break };
                    if framed_json::write_json(
                        &mut writer,
                        &ControlServerFrame::Event {
                        event: Box::new(event),
                    },
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }

    async fn serve_stop_client(stream: super::super::local_endpoint::LocalStream) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = framed_json::reader(read_half);
        let mut writer = framed_json::writer(write_half);
        let Some(hello) = framed_json::read_json::<_, HostHelloRequest>(&mut reader)
            .await
            .expect("read hello")
        else {
            return;
        };
        assert_eq!(hello.token, TOKEN);
        framed_json::write_json(
            &mut writer,
            &HostHelloResponse::accept_control(RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                remote_operations_ready: true,
            }),
        )
        .await
        .expect("write hello");

        let mut active = true;
        while let Some(frame) = framed_json::read_json::<_, ControlClientFrame>(&mut reader)
            .await
            .expect("read control frame")
        {
            match frame {
                ControlClientFrame::Snapshot { refresh_id } => {
                    framed_json::write_json(
                        &mut writer,
                        &ControlServerFrame::Snapshot {
                            refresh_id,
                            account_user_id: None,
                            account_epoch: 1,
                            remote_operations_ready: true,
                            auth: AuthEvent::Ready {
                                user_id: None,
                                account_epoch: 1,
                            },
                            sessions: active
                                .then(|| {
                                    sample_local_session_with_id(
                                        "session-1",
                                        RuntimeSessionStatus::Active,
                                        "Mine",
                                    )
                                })
                                .into_iter()
                                .collect(),
                            rooms: Vec::new(),
                        },
                    )
                    .await
                    .expect("write snapshot");
                    if !active {
                        return;
                    }
                }
                ControlClientFrame::Command { .. } => {
                    active = false;
                    framed_json::write_json(
                        &mut writer,
                        &ControlServerFrame::Event {
                            event: Box::new(HostEvent::Session(SessionEvent::Removed {
                                session_id: "session-1".to_owned(),
                            })),
                        },
                    )
                    .await
                    .expect("write removal");
                }
            }
        }
    }

    async fn serve_rejected_stop_with_stale_removal(
        stream: super::super::local_endpoint::LocalStream,
    ) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = framed_json::reader(read_half);
        let mut writer = framed_json::writer(write_half);
        let Some(hello) = framed_json::read_json::<_, HostHelloRequest>(&mut reader)
            .await
            .expect("read hello")
        else {
            return;
        };
        assert_eq!(hello.token, TOKEN);
        framed_json::write_json(
            &mut writer,
            &HostHelloResponse::accept_control(RemoteCommandStatus {
                account_user_id: None,
                account_epoch: 1,
                remote_operations_ready: true,
            }),
        )
        .await
        .expect("write hello");

        let mut first_snapshot = true;
        let mut rejected_request_id: Option<String> = None;
        let mut sent_rejection = false;
        while let Some(frame) = framed_json::read_json::<_, ControlClientFrame>(&mut reader)
            .await
            .expect("read control frame")
        {
            match frame {
                ControlClientFrame::Snapshot { refresh_id } => {
                    if first_snapshot {
                        first_snapshot = false;
                        framed_json::write_json(
                            &mut writer,
                            &ControlServerFrame::Event {
                                event: Box::new(HostEvent::Session(SessionEvent::Removed {
                                    session_id: "session-1".to_owned(),
                                })),
                            },
                        )
                        .await
                        .expect("write stale removal");
                    } else if !sent_rejection {
                        let request_id = rejected_request_id
                            .as_ref()
                            .expect("stop command precedes confirmation snapshot");
                        for (event_request_id, message) in [
                            ("another-request", "unrelated same-session rejection"),
                            (request_id.as_str(), "stop rejected"),
                        ] {
                            framed_json::write_json(
                                &mut writer,
                                &ControlServerFrame::Event {
                                    event: Box::new(HostEvent::Session(SessionEvent::Error {
                                        operation: "session.stop".to_owned(),
                                        session_id: Some("session-1".to_owned()),
                                        request_id: Some(event_request_id.to_owned()),
                                        message: message.to_owned(),
                                    })),
                                },
                            )
                            .await
                            .expect("write stop error");
                        }
                        sent_rejection = true;
                    }
                    framed_json::write_json(
                        &mut writer,
                        &ControlServerFrame::Snapshot {
                            refresh_id,
                            account_user_id: None,
                            account_epoch: 1,
                            remote_operations_ready: true,
                            auth: AuthEvent::Ready {
                                user_id: None,
                                account_epoch: 1,
                            },
                            sessions: vec![sample_local_session_with_id(
                                "session-1",
                                RuntimeSessionStatus::Active,
                                "Mine",
                            )],
                            rooms: Vec::new(),
                        },
                    )
                    .await
                    .expect("write active snapshot");
                }
                ControlClientFrame::Command { command } => {
                    let command =
                        serde_json::from_value::<HostCommand>(command).expect("typed command");
                    let HostCommand::Session(SessionCommand::Stop { request_id, .. }) = command
                    else {
                        panic!("expected stop command")
                    };
                    rejected_request_id = Some(request_id);
                }
            }
        }
    }

    async fn connect(path: &std::path::Path) -> HeadlessHostClient {
        let stream = connect_local(path).await.expect("connect to the host");
        let mut framed = framed_json::framed(stream);
        framed_json::send_json(
            &mut framed,
            &HostHelloRequest {
                token: TOKEN.to_owned(),
                protocol_version: HOST_PROTOCOL_VERSION,
                lane: ConnectionLane::Control,
            },
        )
        .await
        .expect("send hello");
        let response: HostHelloResponse = framed_json::next_json(&mut framed)
            .await
            .expect("read hello response")
            .expect("the host must answer");
        assert!(response.accepted, "the host must accept a valid hello");
        HeadlessHostClient {
            framed,
            account_user_id: response
                .account_user_id
                .as_deref()
                .map(kodosi_domain::ids::UserId::try_from)
                .transpose()
                .expect("valid hello account"),
            account_epoch: response.account_epoch.expect("control hello account epoch"),
            remote_operations_ready: response
                .remote_operations_ready
                .expect("control hello readiness"),
            pending_events: std::collections::VecDeque::new(),
        }
    }

    async fn assert_snapshot_replaces_hello_readiness(hello_ready: bool, snapshot_ready: bool) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let (events_tx, _events_rx) = broadcast::channel::<HostEvent>(1);
        let host_events = events_tx.clone();
        let host = tokio::spawn(async move {
            while let Ok(stream) = accept_local(&listener).await {
                tokio::spawn(serve_client(
                    stream,
                    host_events.clone(),
                    hello_ready,
                    snapshot_ready,
                    false,
                ));
            }
        });

        let mut client = connect(&socket).await;
        assert_eq!(client.remote_operations_ready(), hello_ready);
        client.snapshot().await.expect("correlated snapshot");
        assert_eq!(client.remote_operations_ready(), snapshot_ready);

        host.abort();
    }

    #[tokio::test]
    async fn correlated_snapshot_replaces_stale_hello_readiness_in_both_directions() {
        assert_snapshot_replaces_hello_readiness(false, true).await;
        assert_snapshot_replaces_hello_readiness(true, false).await;
    }

    #[tokio::test]
    async fn stopping_a_non_resumable_session_settles_on_authoritative_removal() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let host = tokio::spawn(async move {
            let stream = accept_local(&listener).await.expect("accept client");
            serve_stop_client(stream).await;
        });

        let mut client = connect(&socket).await;
        client
            .stop_session("session-1")
            .await
            .expect("authoritative removal settles stop");
        host.await.expect("host task");
    }

    #[tokio::test]
    async fn stale_removal_cannot_hide_a_request_correlated_stop_rejection() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let host = tokio::spawn(async move {
            let stream = accept_local(&listener).await.expect("accept client");
            serve_rejected_stop_with_stale_removal(stream).await;
        });

        let mut client = connect(&socket).await;
        let error = client
            .stop_session("session-1")
            .await
            .expect_err("correlated rejection must win over stale removal");
        assert_eq!(error.to_string(), "unsupported operation: stop rejected");
        drop(client);
        host.await.expect("host task");
    }

    #[tokio::test]
    async fn event_received_before_correlated_snapshot_is_replayed_afterward() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let (events_tx, _events_rx) = broadcast::channel::<HostEvent>(1);
        let host_events = events_tx.clone();
        let host = tokio::spawn(async move {
            while let Ok(stream) = accept_local(&listener).await {
                tokio::spawn(serve_client(stream, host_events.clone(), true, true, true));
            }
        });

        let mut client = connect(&socket).await;
        client.snapshot().await.expect("correlated snapshot");
        let retained = client
            .recv()
            .await
            .expect("receive retained event")
            .expect("retained event exists");
        assert!(matches!(
            retained,
            HostEvent::Session(SessionEvent::Error {
                operation,
                session_id: Some(session_id),
                request_id: None,
                message,
            }) if operation == "session.stop"
                && session_id == OTHER_SESSION
                && message == "cannot stop a session this device does not own"
        ));

        host.abort();
    }

    #[tokio::test]
    async fn a_snapshot_survives_another_clients_broadcast_failure() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let (events_tx, _events_rx) = broadcast::channel::<HostEvent>(64);

        let host_events = events_tx.clone();
        let host = tokio::spawn(async move {
            while let Ok(stream) = accept_local(&listener).await {
                tokio::spawn(serve_client(stream, host_events.clone(), true, true, false));
            }
        });

        let mut snapshotting = connect(&socket).await;
        let _failing = connect(&socket).await;

        let noisy = events_tx.clone();
        let noise = tokio::spawn(async move {
            for _ in 0..8 {
                if noisy.send(other_clients_failure()).is_err() {
                    return;
                }
                time::sleep(Duration::from_millis(5)).await;
            }
        });

        let snapshot = time::timeout(Duration::from_secs(10), snapshotting.snapshot())
            .await
            .expect("the snapshot must be bounded")
            .expect("another client's failure must not fail this snapshot");

        std::assert_matches!(snapshot.auth, HeadlessHostAuthState::Ready);
        assert_eq!(
            snapshot
                .sessions
                .iter()
                .map(super::super::super::SessionListEntry::id)
                .collect::<Vec<_>>(),
            vec!["session-1"],
            "the snapshot must carry the state the host reported"
        );

        noise.abort();
        host.abort();
    }
}

#[test]
fn snapshot_applies_runtime_catalog_replacement_contract() {
    let previous = vec![
        sample_local_session_with_id("session-1", RuntimeSessionStatus::Active, "Local"),
        sample_owned_remote_session(),
    ];
    let current = vec![
        sample_owned_remote_session(),
        sample_local_session_with_id("session-2", RuntimeSessionStatus::Active, "Added"),
    ];
    let mut snapshot = HeadlessHostState::default();

    apply_snapshot_message(
        &mut snapshot,
        HostEvent::Session(session_catalog_snapshot_event(previous)),
    )
    .unwrap_or_else(|error| panic!("initial session snapshot should apply: {error}"));
    apply_snapshot_message(
        &mut snapshot,
        HostEvent::Session(session_catalog_snapshot_event(current.clone())),
    )
    .unwrap_or_else(|error| panic!("replacement session snapshot should apply: {error}"));

    assert_eq!(snapshot.sessions, current);
}

#[test]
fn snapshot_replaces_room_catalog_from_runtime_contract() {
    let mut snapshot = HeadlessHostState::default();

    apply_snapshot_message(
        &mut snapshot,
        HostEvent::Session(room_catalog_event(&[room("room-1", "Old")])),
    )
    .unwrap_or_else(|error| panic!("initial room snapshot should apply: {error}"));
    apply_snapshot_message(
        &mut snapshot,
        HostEvent::Session(room_catalog_event(&[
            room("room-2", "New"),
            room("room-3", "Other"),
        ])),
    )
    .unwrap_or_else(|error| panic!("replacement room snapshot should apply: {error}"));

    assert_eq!(
        snapshot.rooms,
        vec![room("room-2", "New"), room("room-3", "Other")]
    );
}

#[test]
fn hello_request_serialises_and_deserialises() {
    use super::handshake::{
        ConnectionLane, HOST_PROTOCOL_VERSION, HostHelloRequest, HostHelloResponse,
    };

    let req = HostHelloRequest {
        token: "tok-abc".to_owned(),
        protocol_version: HOST_PROTOCOL_VERSION,
        lane: ConnectionLane::Control,
    };
    let json = serde_json::to_string(&req).expect("serialise hello request");
    let round: HostHelloRequest = serde_json::from_str(&json).expect("deserialise hello request");
    assert_eq!(round.token, req.token);
    assert_eq!(round.protocol_version, HOST_PROTOCOL_VERSION);
    assert_eq!(round.lane, ConnectionLane::Control);

    let ok = HostHelloResponse::accept();
    assert!(ok.accepted);
    assert_eq!(ok.server_version, HOST_PROTOCOL_VERSION);

    let err = HostHelloResponse::reject("bad token");
    assert!(!err.accepted);
    assert_eq!(err.message.as_deref(), Some("bad token"));
}

#[test]
fn hello_request_missing_lane_defaults_to_control() {
    use super::handshake::{ConnectionLane, HOST_PROTOCOL_VERSION, HostHelloRequest};

    let json = serde_json::json!({
        "token": "tok",
        "protocol_version": HOST_PROTOCOL_VERSION,
    });
    let req: HostHelloRequest =
        serde_json::from_value(json).expect("deserialise without lane field");
    assert_eq!(req.lane, ConnectionLane::Control);
}

#[test]
fn terminal_lane_serialises_with_session_id() {
    use super::handshake::{ConnectionLane, HOST_PROTOCOL_VERSION, HostHelloRequest};

    let req = HostHelloRequest {
        token: "t".to_owned(),
        protocol_version: HOST_PROTOCOL_VERSION,
        lane: ConnectionLane::Terminal {
            session_id: "sess-1".to_owned(),
            capture: false,
        },
    };
    let json = serde_json::to_string(&req).expect("serialise terminal lane");
    let round: HostHelloRequest = serde_json::from_str(&json).expect("deserialise terminal lane");
    assert_eq!(
        round.lane,
        ConnectionLane::Terminal {
            session_id: "sess-1".to_owned(),
            capture: false
        }
    );
}

mod share_scope_wait {
    use super::super::client::{ScopeWait, ScopeWaitStep};
    use crate::{AppError, HostEvent, SessionEvent, SystemEvent};
    use kodosi_domain::permissions::ShareScope;
    use std::time::Duration;
    use tokio::time;

    const REQUEST: &str = "req-scope";
    const SESSION: &str = "019d1bd1-c0ae-72a0-88cb-c8519739adaa";

    const HOST_BUDGET: Duration = Duration::from_secs(505);

    fn wait() -> ScopeWait {
        ScopeWait::new(REQUEST.to_owned(), SESSION.to_owned())
    }

    fn accepted(request_id: &str, budget: Duration) -> HostEvent {
        HostEvent::Session(SessionEvent::ScopeAccepted {
            request_id: request_id.to_owned(),
            session_id: SESSION.to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            scope: ShareScope::MyDevices,
            room_id: None,
            budget_ms: u64::try_from(budget.as_millis()).expect("test budget fits"),
        })
    }

    fn changed(request_id: &str) -> HostEvent {
        HostEvent::Session(SessionEvent::ScopeChanged {
            request_id: request_id.to_owned(),
            session_id: SESSION.to_owned(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            scope: ShareScope::MyDevices,
            room_id: None,
        })
    }

    fn failed(request_id: Option<&str>, message: &str) -> HostEvent {
        HostEvent::Session(SessionEvent::Error {
            operation: "session.scope".to_owned(),
            session_id: Some(SESSION.to_owned()),
            request_id: request_id.map(ToOwned::to_owned),
            message: message.to_owned(),
        })
    }

    fn heartbeat() -> HostEvent {
        HostEvent::System(SystemEvent::Heartbeat)
    }

    #[tokio::test(start_paused = true)]
    async fn an_unaccepted_request_is_bounded_by_host_silence() {
        let wait = wait();

        assert_eq!(
            wait.deadline() - time::Instant::now(),
            Duration::from_secs(15)
        );
        std::assert_matches!(
            wait.gave_up(),
            AppError::Unsupported { ref reason } if reason.contains("never accepted")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeats_cannot_extend_an_unaccepted_requests_deadline() {
        let mut wait = wait();
        let admission_deadline = wait.deadline();

        for _ in 0..24 {
            time::advance(Duration::from_secs(5)).await;
            std::assert_matches!(wait.observe(&heartbeat()), ScopeWaitStep::Pending);
            assert_eq!(wait.deadline(), admission_deadline);
        }

        assert!(time::Instant::now() >= wait.deadline());
        std::assert_matches!(
            wait.gave_up(),
            AppError::Unsupported { ref reason } if reason.contains("never accepted")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn acceptance_hands_the_bound_to_the_budget_the_host_published() {
        let mut wait = wait();

        std::assert_matches!(
            wait.observe(&accepted(REQUEST, HOST_BUDGET)),
            ScopeWaitStep::Pending
        );

        assert_eq!(wait.deadline() - time::Instant::now(), HOST_BUDGET);
        std::assert_matches!(
            wait.gave_up(),
            AppError::Unsupported { ref reason } if reason.contains("its own 505s budget")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn work_longer_than_the_old_fifteen_second_wait_still_settles() {
        let mut wait = wait();
        drop(wait.observe(&accepted(REQUEST, HOST_BUDGET)));

        time::advance(Duration::from_mins(1)).await;
        assert!(time::Instant::now() < wait.deadline());

        std::assert_matches!(wait.observe(&changed(REQUEST)), ScopeWaitStep::Settled);
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeats_after_acceptance_do_not_extend_the_hosts_budget() {
        let mut wait = wait();
        drop(wait.observe(&accepted(REQUEST, HOST_BUDGET)));
        let settled_by = wait.deadline();

        time::advance(Duration::from_secs(30)).await;
        std::assert_matches!(wait.observe(&heartbeat()), ScopeWaitStep::Pending);

        assert_eq!(
            wait.deadline(),
            settled_by,
            "liveness must not soften a bound the runtime already committed to"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_hosts_failure_for_this_request_is_the_verdict() {
        let mut wait = wait();
        drop(wait.observe(&accepted(REQUEST, HOST_BUDGET)));

        let step = wait.observe(&failed(
            Some(REQUEST),
            "friends scope needs an audience manifest",
        ));

        std::assert_matches!(
            step,
            ScopeWaitStep::Failed(AppError::Unsupported { ref reason })
                if reason.contains("audience manifest") && reason.contains("session.scope")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejection_before_acceptance_is_still_correlated() {
        let mut wait = wait();

        let step = wait.observe(&failed(Some(REQUEST), "sessionId must not be empty"));

        std::assert_matches!(
            step,
            ScopeWaitStep::Failed(AppError::Unsupported { ref reason })
                if reason.contains("must not be empty")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_failure_for_another_request_does_not_settle_this_one() {
        let mut wait = wait();
        drop(wait.observe(&accepted(REQUEST, HOST_BUDGET)));

        std::assert_matches!(
            wait.observe(&failed(Some("some-other-request"), "unrelated")),
            ScopeWaitStep::Pending
        );
        std::assert_matches!(wait.observe(&changed(REQUEST)), ScopeWaitStep::Settled);
    }

    #[tokio::test(start_paused = true)]
    async fn another_requests_verdict_does_not_settle_this_one() {
        let mut wait = wait();
        drop(wait.observe(&accepted(REQUEST, HOST_BUDGET)));

        std::assert_matches!(
            wait.observe(&changed("some-other-request")),
            ScopeWaitStep::Pending
        );
        std::assert_matches!(
            wait.observe(&accepted("some-other-request", Duration::from_secs(1))),
            ScopeWaitStep::Pending
        );
        assert_eq!(
            wait.deadline() - time::Instant::now(),
            HOST_BUDGET,
            "another request's budget must not become this one's bound"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_uncorrelated_scope_failure_for_this_session_is_still_reported() {
        let mut wait = wait();

        let step = wait.observe(&failed(None, "roomId is required for room scope"));

        std::assert_matches!(
            step,
            ScopeWaitStep::Failed(AppError::Unsupported { ref reason })
                if reason.contains("roomId is required")
        );
    }
}

mod host_stop {
    use std::{path::Path, time::Duration};

    use tokio::time;

    use super::super::{
        handshake::{
            ConnectionLane, ControlClientFrame, HOST_PROTOCOL_VERSION, HostHelloRequest,
            HostHelloResponse,
        },
        lifecycle::{
            HostConnectOutcome, classify_control_connect_error, connect_control_lane, stop_host_in,
        },
        local_endpoint::{
            accept_local, acquire_host_lock, bind_local, connect_local, socket_path_for_dir,
        },
        server::stop_serving,
        state_file::{
            HOST_CONTRACT_VERSION, HostStateFile, host_state_path_in, load_host_state_file_in,
        },
    };
    use crate::{
        AppError, HostCommand, SystemCommand, support::io::framed_json,
        support::platform::fs as support_fs,
    };

    const NO_HANG: Duration = Duration::from_mins(2);

    const TEARDOWN_DELAY: Duration = Duration::from_millis(50);

    fn publish(runtime_dir: &Path, socket_path: &Path, token: &str) -> HostStateFile {
        let state = HostStateFile {
            version: HOST_CONTRACT_VERSION,
            pid: std::process::id(),
            socket_path: socket_path.to_string_lossy().into_owned(),
            token: token.to_owned(),
            config_identity: Some(
                crate::config::configuration_identity().expect("config identity"),
            ),
            started_at: "2026-07-26T00:00:00Z".to_owned(),
        };
        crate::support::storage::atomic_file::atomic_write_json(
            &host_state_path_in(runtime_dir),
            &state,
            true,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
        .expect("publish the host record");
        state
    }

    fn runtime_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp runtime dir");
        support_fs::ensure_dir(dir.path()).expect("prepare temp runtime dir");
        dir
    }

    async fn answer_hello(
        stream: super::super::local_endpoint::LocalStream,
        accept_token: &str,
    ) -> Option<framed_json::FramedJsonStream<super::super::local_endpoint::LocalStream>> {
        let mut framed = framed_json::framed(stream);
        let request: HostHelloRequest = framed_json::next_json(&mut framed)
            .await
            .expect("read hello")?;
        let response = if request.token == accept_token {
            HostHelloResponse::accept()
        } else {
            HostHelloResponse::reject("invalid host token")
        };
        framed_json::send_json(&mut framed, &response)
            .await
            .expect("write hello response");
        (request.token == accept_token).then_some(framed)
    }

    #[tokio::test(start_paused = true)]
    async fn a_listener_that_never_accepts_is_reported_silent_instead_of_parking_the_client() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());

        let _listener = bind_local(&socket).await.expect("bind host socket");
        publish(dir.path(), &socket, "token");

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("the handshake must be bounded")
            .expect("handshake should not error");

        std::assert_matches!(outcome, HostConnectOutcome::Silent);
    }

    #[tokio::test(start_paused = true)]
    async fn a_host_that_accepts_but_never_answers_the_hello_is_reported_silent() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        publish(dir.path(), &socket, "token");

        let stalled = tokio::spawn(async move {
            let stream = accept_local(&listener).await.expect("accept");

            std::future::pending::<()>().await;
            drop(stream);
        });

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("a stalled handshake must be bounded")
            .expect("handshake should not error");

        std::assert_matches!(outcome, HostConnectOutcome::Silent);
        stalled.abort();
    }

    #[tokio::test]
    async fn a_saturated_backlog_timeout_preserves_the_live_host_record() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        let state = publish(dir.path(), &socket, "token");

        let outcome = classify_control_connect_error(
            dir.path(),
            &state,
            &socket,
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "simulated saturated listener backlog",
            ),
        )
        .expect("connect timeout should be classified");

        std::assert_matches!(outcome, HostConnectOutcome::Silent);
        assert_eq!(
            load_host_state_file_in(dir.path())
                .expect("read record")
                .as_ref()
                .map(|current| current.token.as_str()),
            Some("token"),
            "a connect timeout proves silence, not that the host is missing"
        );
    }

    #[tokio::test]
    async fn closing_the_listener_releases_a_client_already_parked_on_the_handshake() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        publish(dir.path(), &socket, "token");

        let teardown_dir = dir.path().to_path_buf();
        let teardown_socket = socket.clone();
        tokio::spawn(async move {
            time::sleep(TEARDOWN_DELAY).await;
            stop_serving(&teardown_dir, &teardown_socket, listener);
        });

        let started = time::Instant::now();
        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("the handshake must be bounded")
            .expect("handshake should not error");

        std::assert_matches!(outcome, HostConnectOutcome::Gone);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the client must be released by the listener closing, not by its own timeout"
        );
        assert!(
            !host_state_path_in(dir.path()).exists(),
            "a host that stops serving clears its record first"
        );
    }

    #[tokio::test]
    async fn a_record_pointing_at_a_dead_socket_is_cleared_as_stale() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());

        drop(bind_local(&socket).await.expect("bind then abandon"));
        let state = publish(dir.path(), &socket, "token");

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("a dead socket must fail fast")
            .expect("handshake should not error");

        std::assert_matches!(outcome, HostConnectOutcome::Gone);
        assert!(
            load_host_state_file_in(dir.path())
                .expect("read record")
                .is_none(),
            "a record that points at nothing must not survive"
        );
        drop(state);
    }

    #[tokio::test]
    async fn a_record_naming_something_that_is_not_a_socket_is_cleared_as_stale() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        std::fs::write(&socket, b"not a socket").expect("write a corrupted socket path");
        publish(dir.path(), &socket, "token");

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("a corrupted record must fail fast")
            .expect("handshake should not error");

        std::assert_matches!(outcome, HostConnectOutcome::Gone);
        assert!(
            load_host_state_file_in(dir.path())
                .expect("read record")
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_record_replaced_by_a_new_host_survives_a_failed_connect_to_the_old_one() {
        let dir = runtime_dir();
        let dead_socket = dir.path().join("dead.sock");
        let stale = publish(dir.path(), &dead_socket, "old-token");
        let live_socket = socket_path_for_dir(dir.path());
        let _listener = bind_local(&live_socket).await.expect("bind replacement");
        let replacement = publish(dir.path(), &live_socket, "new-token");

        super::super::state_file::cleanup_host_state_file_if_matches(dir.path(), &stale);

        let current = load_host_state_file_in(dir.path())
            .expect("read record")
            .expect("the replacement record must survive");
        assert_eq!(current.token, replacement.token);
    }

    #[tokio::test]
    async fn a_token_replaced_mid_handshake_is_retried_with_the_published_one() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        publish(dir.path(), &socket, "stale-token");

        let host_dir = dir.path().to_path_buf();
        let host_socket = socket.clone();
        let host = tokio::spawn(async move {
            let stream = accept_local(&listener).await.expect("accept first");
            publish(&host_dir, &host_socket, "live-token");
            drop(answer_hello(stream, "live-token").await);

            let stream = accept_local(&listener).await.expect("accept second");
            answer_hello(stream, "live-token").await.is_some()
        });

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("the retry must be bounded")
            .expect("handshake should not error");

        std::assert_matches!(outcome, HostConnectOutcome::Connected(_));
        assert!(host.await.expect("host task joined"));
    }

    struct FakeHost {
        handle: tokio::task::JoinHandle<()>,
    }

    impl FakeHost {
        async fn start(runtime_dir: &Path, token: &'static str, drain: Duration) -> Self {
            let dir = runtime_dir.to_path_buf();
            let socket = socket_path_for_dir(runtime_dir);
            let lock = acquire_host_lock(runtime_dir).expect("host takes the runtime lock");
            let listener = bind_local(&socket).await.expect("bind host socket");
            publish(runtime_dir, &socket, token);

            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
            let handle = tokio::spawn(async move {
                let _lock = lock;
                if ready_tx.send(()).is_err() {
                    return;
                }
                let mut clients = Vec::new();
                loop {
                    let stream = match accept_local(&listener).await {
                        Ok(stream) => stream,
                        Err(_) => break,
                    };
                    let Some(mut framed) = answer_hello(stream, token).await else {
                        continue;
                    };
                    let Ok(Some(frame)) =
                        framed_json::next_json::<_, ControlClientFrame>(&mut framed).await
                    else {
                        clients.push(framed);
                        continue;
                    };
                    let ControlClientFrame::Command { command } = frame else {
                        clients.push(framed);
                        continue;
                    };
                    let Ok(command) = serde_json::from_value::<HostCommand>(command) else {
                        clients.push(framed);
                        continue;
                    };
                    if matches!(command, HostCommand::System(SystemCommand::Shutdown)) {
                        break;
                    }
                    clients.push(framed);
                }
                stop_serving(&dir, &socket, listener);
                drop(clients);

                time::sleep(drain).await;
            });
            ready_rx.await.expect("host task started");
            Self { handle }
        }
    }

    #[tokio::test]
    async fn stop_reports_success_only_once_the_host_has_released_its_runtime_lock() {
        let dir = runtime_dir();
        let host = FakeHost::start(dir.path(), "token", Duration::from_millis(400)).await;

        let stopped = time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("stop must be bounded")
            .expect("stop should succeed");

        assert!(stopped, "a running host must be reported as stopped");

        drop(acquire_host_lock(dir.path()).expect("the runtime lock must be free after stop"));
        assert!(
            load_host_state_file_in(dir.path())
                .expect("read record")
                .is_none()
        );
        host.handle.await.expect("host task joined");
    }

    #[tokio::test(start_paused = true)]
    async fn stop_is_idempotent_when_no_host_is_running() {
        let dir = runtime_dir();

        let stopped = time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("stop must be bounded")
            .expect("stop should succeed");

        assert!(!stopped, "no host means nothing was stopped");
    }

    #[tokio::test]
    async fn concurrent_stops_both_settle_and_neither_hangs() {
        let dir = runtime_dir();
        let host = FakeHost::start(dir.path(), "token", Duration::from_millis(400)).await;

        let (first, second) = time::timeout(
            NO_HANG,
            futures_util::future::join(stop_host_in(dir.path()), stop_host_in(dir.path())),
        )
        .await
        .expect("concurrent stops must be bounded");

        let first = first.expect("first stop should settle");
        let second = second.expect("second stop should settle");
        assert!(
            first || second,
            "at least one caller must report that a host was running"
        );
        drop(acquire_host_lock(dir.path()).expect("the runtime lock must be free after stop"));
        host.handle.await.expect("host task joined");
    }

    #[tokio::test(start_paused = true)]
    async fn stop_fails_with_an_actionable_verdict_when_the_host_is_wedged() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());

        let _lock = acquire_host_lock(dir.path()).expect("wedged host holds the lock");
        let _listener = bind_local(&socket).await.expect("bind host socket");
        publish(dir.path(), &socket, "token");

        let error = time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("a wedged host must not hang the caller")
            .expect_err("a wedged host must not be reported as stopped");

        std::assert_matches!(
            error,
            AppError::Unsupported { ref reason } if reason.contains("wedged"),
            "the verdict must name the failure, got: {error}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_terminal_lane_connect_to_a_silent_host_is_bounded_too() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        let _listener = bind_local(&socket).await.expect("bind host socket");

        let connected = time::timeout(NO_HANG, connect_local(&socket))
            .await
            .expect("connect must be bounded")
            .expect("connect should succeed into the backlog");
        let mut framed = framed_json::framed(connected);
        framed_json::send_json(
            &mut framed,
            &HostHelloRequest {
                token: "token".to_owned(),
                protocol_version: HOST_PROTOCOL_VERSION,
                lane: ConnectionLane::Terminal {
                    session_id: "019d1bd1-c0ae-72a0-88cb-c8519739adaa".to_owned(),
                    capture: true,
                },
            },
        )
        .await
        .expect("send terminal hello");

        let answer = time::timeout(
            Duration::from_secs(30),
            framed_json::next_json::<_, HostHelloResponse>(&mut framed),
        )
        .await;
        assert!(
            answer.is_err(),
            "an unaccepted connection answers nothing; only a deadline ends the wait"
        );
    }
}

#[cfg(unix)]
mod protocol_version_recovery {
    use std::{path::Path, time::Duration};

    use tokio::time;

    use super::super::{
        handshake::{
            ControlClientFrame, HOST_PROTOCOL_VERSION, HostHelloRequest, HostHelloResponse,
        },
        lifecycle::{HostConnectOutcome, classify_rejection, connect_control_lane, stop_host_in},
        local_endpoint::{accept_local, acquire_host_lock, bind_local, socket_path_for_dir},
        server::stop_serving,
        state_file::{
            HOST_CONTRACT_VERSION, HostStateFile, host_state_path_in, load_host_state_file_in,
        },
    };
    use crate::{
        AppError, HostCommand, SystemCommand, support::io::framed_json,
        support::platform::fs as support_fs,
    };

    const NO_HANG: Duration = Duration::from_mins(2);

    const OLD_VERSION: u8 = HOST_PROTOCOL_VERSION - 1;
    const TOKEN: &str = "token";

    fn runtime_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp runtime dir");
        support_fs::ensure_dir(dir.path()).expect("prepare temp runtime dir");
        dir
    }

    fn publish(runtime_dir: &Path, socket_path: &Path, token: &str) -> HostStateFile {
        let state = HostStateFile {
            version: HOST_CONTRACT_VERSION,
            pid: std::process::id(),
            socket_path: socket_path.to_string_lossy().into_owned(),
            token: token.to_owned(),
            config_identity: Some(
                crate::config::configuration_identity().expect("config identity"),
            ),
            started_at: "2026-07-26T00:00:00Z".to_owned(),
        };
        crate::support::storage::atomic_file::atomic_write_json(
            &host_state_path_in(runtime_dir),
            &state,
            true,
            crate::support::storage::atomic_file::FileMode::UserPrivate,
        )
        .expect("publish the host record");
        state
    }

    fn reject_with_version(server_version: u8, message: &str) -> HostHelloResponse {
        HostHelloResponse {
            accepted: false,
            server_version,
            message: Some(message.to_owned()),
            capability: None,
            account_user_id: None,
            account_epoch: None,
            remote_operations_ready: None,
        }
    }

    struct OldHost {
        handle: tokio::task::JoinHandle<bool>,
    }

    impl OldHost {
        async fn start(runtime_dir: &Path, honours_shutdown: bool) -> Self {
            let dir = runtime_dir.to_path_buf();
            let socket = socket_path_for_dir(runtime_dir);
            let lock = acquire_host_lock(runtime_dir).expect("the old host takes the runtime lock");
            let listener = bind_local(&socket).await.expect("bind the old host socket");
            publish(runtime_dir, &socket, TOKEN);

            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
            let handle = tokio::spawn(async move {
                let _lock = lock;
                if ready_tx.send(()).is_err() {
                    return false;
                }
                let mut asked_to_stop = false;
                let mut clients = Vec::new();
                loop {
                    let Ok(stream) = accept_local(&listener).await else {
                        break;
                    };
                    let mut framed = framed_json::framed(stream);
                    let Ok(Some(hello)) =
                        framed_json::next_json::<_, HostHelloRequest>(&mut framed).await
                    else {
                        continue;
                    };

                    if hello.token != TOKEN {
                        drop(
                            framed_json::send_json(
                                &mut framed,
                                &reject_with_version(OLD_VERSION, "invalid host token"),
                            )
                            .await,
                        );
                        continue;
                    }
                    if hello.protocol_version != OLD_VERSION || !honours_shutdown {
                        drop(
                            framed_json::send_json(
                                &mut framed,
                                &reject_with_version(
                                    OLD_VERSION,
                                    &format!(
                                        "protocol version mismatch: client={} server={OLD_VERSION}",
                                        hello.protocol_version
                                    ),
                                ),
                            )
                            .await,
                        );
                        continue;
                    }
                    drop(framed_json::send_json(&mut framed, &HostHelloResponse::accept()).await);
                    let shutdown = if OLD_VERSION >= 11 {
                        match framed_json::next_json::<_, ControlClientFrame>(&mut framed).await {
                            Ok(Some(ControlClientFrame::Command { command })) => {
                                serde_json::from_value::<HostCommand>(command).ok()
                            }
                            _ => None,
                        }
                    } else {
                        framed_json::next_json::<_, HostCommand>(&mut framed)
                            .await
                            .ok()
                            .flatten()
                    };
                    match shutdown {
                        Some(HostCommand::System(SystemCommand::Shutdown)) => {
                            asked_to_stop = true;
                            break;
                        }
                        _ => clients.push(framed),
                    }
                }
                stop_serving(&dir, &socket, listener);
                drop(clients);
                asked_to_stop
            });
            ready_rx.await.expect("the old host started");
            Self { handle }
        }
    }

    #[tokio::test]
    async fn a_host_on_another_protocol_is_reported_incompatible_not_merely_rejected() {
        assert_eq!(
            OLD_VERSION,
            HOST_PROTOCOL_VERSION - 1,
            "the simulated peer must remain exactly one incompatible version behind"
        );
        let dir = runtime_dir();
        let host = OldHost::start(dir.path(), true).await;

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("the handshake must be bounded")
            .expect("handshake should not error");

        std::assert_matches!(
            outcome,
            HostConnectOutcome::Incompatible { server_version, .. }
                if server_version == OLD_VERSION,
        );

        drop(stop_host_in(dir.path()).await);
        drop(host.handle.await);
    }

    #[tokio::test]
    async fn stopping_an_incompatible_host_asks_it_to_leave_in_its_own_version() {
        let dir = runtime_dir();
        let host = OldHost::start(dir.path(), true).await;

        let stopped = time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("the stop must be bounded")
            .expect("an incompatible host must still be stoppable");

        assert!(stopped, "a running host must be reported as stopped");
        assert!(
            host.handle.await.expect("the old host joined"),
            "the old host must receive a graceful shutdown, not be abandoned",
        );

        drop(acquire_host_lock(dir.path()).expect("the runtime lock must be free after the stop"));
        assert!(
            load_host_state_file_in(dir.path())
                .expect("read record")
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_host_that_cannot_be_asked_at_all_still_yields_a_bounded_version_verdict() {
        let dir = runtime_dir();
        let host = OldHost::start(dir.path(), false).await;

        let error = time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("an unstoppable host must not hang the caller")
            .expect_err("a host that never leaves must not be reported as stopped");

        std::assert_matches!(
            error,
            AppError::Unsupported { ref reason }
                if reason.contains(&format!("v{OLD_VERSION}")) && !reason.contains("wedged"),
            "the verdict must name the version gap, got: {error}",
        );
        host.handle.abort();
    }

    #[tokio::test]
    async fn a_token_refusal_is_never_treated_as_a_version_problem() {
        let dir = runtime_dir();
        let socket = socket_path_for_dir(dir.path());
        let listener = bind_local(&socket).await.expect("bind host socket");
        let _lock = acquire_host_lock(dir.path()).expect("the host holds the runtime lock");

        publish(dir.path(), &socket, "a-token-the-host-does-not-know");

        let host = tokio::spawn(async move {
            while let Ok(stream) = accept_local(&listener).await {
                let mut framed = framed_json::framed(stream);
                let Ok(Some(_hello)) =
                    framed_json::next_json::<_, HostHelloRequest>(&mut framed).await
                else {
                    continue;
                };

                drop(
                    framed_json::send_json(
                        &mut framed,
                        &reject_with_version(HOST_PROTOCOL_VERSION, "invalid host token"),
                    )
                    .await,
                );
            }
        });

        let outcome = time::timeout(NO_HANG, connect_control_lane(dir.path()))
            .await
            .expect("the handshake must be bounded")
            .expect("handshake should not error");
        std::assert_matches!(
            outcome,
            HostConnectOutcome::Rejected(ref reason) if reason.contains("invalid host token"),
        );

        let error = time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("the stop must be bounded")
            .expect_err("a host we could not authenticate against must not be stopped");
        std::assert_matches!(
            error,
            AppError::Unsupported { ref reason } if reason.contains("refused the stop request"),
        );

        host.abort();
    }

    #[tokio::test]
    async fn the_incompatible_host_is_replaced_rather_than_left_blocking_every_command() {
        let dir = runtime_dir();
        let host = OldHost::start(dir.path(), true).await;

        let outcome = connect_control_lane(dir.path())
            .await
            .expect("handshake should not error");
        std::assert_matches!(outcome, HostConnectOutcome::Incompatible { .. });
        time::timeout(NO_HANG, stop_host_in(dir.path()))
            .await
            .expect("the replacement stop must be bounded")
            .expect("the incompatible host must be replaceable");

        assert!(
            host.handle.await.expect("the old host joined"),
            "the old host must be asked to leave, not killed out from under its sessions",
        );
        drop(
            acquire_host_lock(dir.path())
                .expect("a replacement host must be able to take the lock"),
        );
        std::assert_matches!(
            connect_control_lane(dir.path())
                .await
                .expect("handshake should not error"),
            HostConnectOutcome::Gone,
            "nothing must be left claiming to serve this runtime directory",
        );
    }

    #[test]
    fn only_a_differing_server_version_is_classified_incompatible() {
        std::assert_matches!(
            classify_rejection(&reject_with_version(HOST_PROTOCOL_VERSION, "no")),
            HostConnectOutcome::Rejected(_),
        );
        std::assert_matches!(
            classify_rejection(&reject_with_version(OLD_VERSION, "no")),
            HostConnectOutcome::Incompatible { server_version, .. }
                if server_version == OLD_VERSION,
        );

        std::assert_matches!(
            classify_rejection(&reject_with_version(HOST_PROTOCOL_VERSION + 1, "no")),
            HostConnectOutcome::Incompatible { server_version, .. }
                if server_version == HOST_PROTOCOL_VERSION + 1,
        );
    }
}
