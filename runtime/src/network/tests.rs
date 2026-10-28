use super::*;

fn session() -> RemoteSession {
    RemoteSession {
        id: Uuid::now_v7(),
        incarnation_id: Uuid::now_v7(),
        name: "test".into(),
        owner_user_id: Uuid::now_v7().to_string(),
        owner_name: "owner".into(),
        host_device_id: Uuid::now_v7().to_string(),
        host_name: "host".into(),
        mission_id: None,
        mission_name: None,
        shared_with: vec![],
        online: true,
        status: None,
        hidden: false,
        restored_from: None,
    }
}

#[test]
fn a_listed_terminal_names_the_terminal_it_replaces_but_never_itself() {
    let id = Uuid::now_v7();
    let listed = |restored_from: Value| -> RemoteSession {
        let dto = serde_json::json!({
            "id": id, "incarnationId": Uuid::now_v7(), "name": "Build", "ownerUserId": "owner",
            "ownerName": "Owner", "hostDeviceId": "host", "hostName": "Host", "missionId": null,
            "missionName": null, "sharedWith": [], "authorizationRevision": 1, "hostOnline": true,
            "roomSequence": 0, "restoredFrom": restored_from,
        });
        serde_json::from_value::<wire::SessionDto>(dto)
            .unwrap()
            .into()
    };
    let replaced = Uuid::now_v7();
    assert_eq!(
        listed(serde_json::json!(replaced)).restored_from,
        Some(replaced)
    );
    assert_eq!(listed(serde_json::json!(id)).restored_from, None);
    assert_eq!(listed(Value::Null).restored_from, None);
}

#[cfg(feature = "host")]
#[test]
fn the_server_hears_the_agent_and_its_state_but_not_its_text() {
    use crate::terminal::{
        BlockedKind, ContextPlace, PlaceKind, ProgramState, ProgramStatus, TerminalContext,
        TerminalMetadata,
    };
    let status = TerminalStatus::of(&TerminalMetadata {
        connected_users: vec![],
        directory: Some("/Users/someone/private".into()),
        title: Some("A private title".into()),
        program: Some("claude".into()),
        status: Some(ProgramStatus {
            state: ProgramState::Blocked,
            kind: Some(BlockedKind::Permission),
            progress: None,
            title: Some("Bash".into()),
            message: Some("rm -rf build".into()),
        }),
        context: Some(TerminalContext {
            place: Some(ContextPlace {
                kind: PlaceKind::Remote,
                name: "private-server".into(),
            }),
            root: true,
        }),
    });
    let bytes = serde_json::to_vec(&status).unwrap();
    assert_eq!(
        bytes,
        br#"{"program":"claude","state":"blocked","kind":"permission"}"#
    );
    let notice = |status: Value| {
        wire::TerminalNotice::read(&serde_json::json!({"sessionId": Uuid::nil(),
            "incarnationId": Uuid::nil(), "online": true, "status": status}))
    };
    assert_eq!(
        notice(serde_json::from_slice(&bytes).unwrap())
            .unwrap()
            .status,
        Some(status)
    );
    for ignored in [
        serde_json::json!({"program":"other"}),
        serde_json::json!({"state":"working","kind":"permission"}),
    ] {
        assert_eq!(notice(ignored).unwrap().status, None);
    }
    assert!(notice(serde_json::json!({"state":"blocked","message":"Allow?"})).is_err());
}

#[test]
fn terminal_close_has_one_wire_action() {
    let encoded = wire::encode_control(&TerminalControl::Close).unwrap();
    assert_eq!(encoded, br#"{"type":"close"}"#);
    assert!(matches!(
        wire::decode_control(&encoded),
        Ok(TerminalControl::Close)
    ));
    assert!(wire::decode_control(br#"{"type":"stop"}"#).is_err());
    assert!(wire::decode_control(br#"{"type":"input","bytes":"bHM="}"#).is_err());
    assert!(
        wire::encode_control(&TerminalControl::Input {
            bytes: b"ls".to_vec()
        })
        .is_err()
    );
}

#[tokio::test]
async fn independent_control_handle_does_not_block_output() {
    let (mut connection, mut requests, updates) = test_remote_connection(session());
    let control = connection.control_handle();
    let pending = tokio::spawn(async move {
        control
            .send_control(TerminalControl::Input {
                bytes: b"a".to_vec(),
            })
            .await
    });
    let TestRemoteRequest::Control { reply, .. } = requests.recv().await.unwrap() else {
        panic!("Expected control");
    };
    updates
        .send(RemoteUpdate::Raw {
            sequence: 0,
            bytes: Bytes::from_static(b"output"),
        })
        .await
        .unwrap_or_else(|_| panic!("closed output channel"));
    assert!(matches!(
        connection.updates.recv().await,
        Some(RemoteUpdate::Raw { sequence: 0, .. })
    ));
    assert!(!pending.is_finished());
    reply.send(Ok(Value::Null)).unwrap();
    assert!(pending.await.unwrap().is_ok());
}

#[tokio::test]
async fn confirmed_terminal_close_wins_over_immediate_connection_close() {
    let (connection, mut requests, _updates) = test_remote_connection(session());
    let control = connection.control_handle();
    let pending = control.send_control(TerminalControl::Close);
    tokio::pin!(pending);
    assert!(futures_util::poll!(&mut pending).is_pending());
    let TestRemoteRequest::Control { reply, .. } = requests.recv().await.unwrap() else {
        panic!("Expected control");
    };
    reply.send(Ok(Value::Null)).unwrap();
    connection.disconnect();
    assert!(pending.await.is_ok());
}

#[tokio::test]
async fn disconnect_fails_uncertain_input_without_replaying() {
    let (connection, mut requests, _updates) = test_remote_connection(session());
    let control = connection.control_handle();
    let pending =
        tokio::spawn(async move { control.send_control(TerminalControl::Interrupt).await });
    let request = requests.recv().await.unwrap();
    connection.disconnect();
    assert!(pending.await.unwrap().is_err());
    drop(request);
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn a_connection_that_drops_during_a_request_has_a_clear_text() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        drop(tokio::io::AsyncReadExt::read(&mut socket, &mut request).await);
    });
    let error = Error::from(
        reqwest::Client::new()
            .get(format!("http://{address}/v1/sessions"))
            .send()
            .await
            .unwrap_err(),
    );
    server.await.unwrap();
    assert!(matches!(error, Error::Dropped), "{error}");
    assert!(error.unanswered());
    assert!(!error.to_string().contains("http"));
}

#[test]
fn a_retry_waits_a_random_time_between_half_and_all_of_its_delay() {
    let delay = Duration::from_secs(8);
    let waits: BTreeSet<_> = (0..64).map(|_| stagger(delay)).collect();
    assert!(waits.iter().all(|wait| (delay / 2..=delay).contains(wait)));
    assert!(waits.len() > 1);
}
