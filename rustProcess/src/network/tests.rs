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
        room_id: None,
        room_name: None,
        shared_with: vec![],
        online: true,
    }
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
async fn confirmed_stop_wins_over_immediate_connection_close() {
    let (connection, mut requests, _updates) = test_remote_connection(session());
    let control = connection.control_handle();
    let pending = control.send_control(TerminalControl::Stop);
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
