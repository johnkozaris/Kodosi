use super::*;
use crate::network::{CheckpointCut, PublishedFrame, RemoteUpdate, TerminalControl};
use tokio::sync::oneshot;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Fixture {
    pub(super) base_url: String,
    pub(super) owner_token: String,
    pub(super) friend_token: String,
}

pub(super) async fn signed_in(root: &std::path::Path, base: &reqwest::Url, token: &str) -> Network {
    let network = Network::new(NetworkConfig {
        api_url: base.clone(),
        issuer: base.as_str().into(),
        client_id: "isolated-test".into(),
        scopes: vec![],
        audience: None,
        data_root: root.to_owned(),
        secret_service: "isolated-network-test".into(),
        isolated: true,
    })
    .unwrap();
    let mut events = network.events();
    network
        .finish_login(
            Tokens {
                access_token: token.to_owned(),
                refresh_token: None,
                expires_at: identity::now_ms() + 60 * 60_000,
                token_endpoint: base.as_str().into(),
            },
            network.generation(),
        )
        .await
        .unwrap();
    while let Ok(event) = events.try_recv() {
        validate_event(&event.event);
    }
    network
}

pub(super) fn validate_event(value: &Value) {
    if value["type"] == "network.sharing" {
        wire::id(value, "sessionId").unwrap();
        wire::id(value, "incarnationId").unwrap();
        serde_json::from_value::<Vec<String>>(value["sharedWith"].clone()).unwrap();
    } else if value["type"] != "network.sessions" {
        crate::protocol::Event::new(None, 0, value.clone())
            .unwrap_or_else(|error| panic!("Desktop event {} is invalid: {error}", value["type"]));
    }
}

pub(super) async fn execute(network: &Network, operation: &str, args: Value) -> NetworkReply {
    let reply = network.execute(operation, args).await.unwrap();
    for event in &reply.events {
        validate_event(event);
    }
    reply
}

pub(super) async fn ready(network: &Network, id: Uuid) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let dto: SessionDto = network
                .inner
                .http
                .device(
                    Method::GET,
                    &format!("api/sessions/{id}"),
                    &network.credentials().unwrap(),
                    None,
                )
                .await
                .unwrap();
            if dto.ready && dto.host_online {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
}

#[expect(
    clippy::too_many_lines,
    reason = "single end-to-end account enrollment, sharing and revocation journey"
)]
#[tokio::test]
#[ignore = "requires KODOSI_NETWORK_TEST_FIXTURE pointing to a disposable loopback signed-OIDC backend"]
async fn signed_backend_sharing_round_trip() {
    let path = std::env::var("KODOSI_NETWORK_TEST_FIXTURE").unwrap();
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let base_url = reqwest::Url::parse(&fixture.base_url).unwrap();
    assert!(base_url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    }));
    let root = tempfile::tempdir().unwrap();
    let owner = signed_in(&root.path().join("owner"), &base_url, &fixture.owner_token).await;
    let friend = signed_in(
        &root.path().join("friend"),
        &base_url,
        &fixture.friend_token,
    )
    .await;
    assert!(owner.identity().unwrap().enrolled);
    assert!(friend.identity().unwrap().enrolled);
    let owner_profile: Value = owner
        .inner
        .http
        .bearer(Method::GET, "api/me", &fixture.owner_token, None)
        .await
        .unwrap();
    let friend_profile: Value = friend
        .inner
        .http
        .bearer(Method::GET, "api/me", &fixture.friend_token, None)
        .await
        .unwrap();
    owner
        .execute(
            "friends.request.send",
            json!({"username":friend_profile["handle"]}),
        )
        .await
        .unwrap();
    friend
        .execute(
            "friends.request.accept",
            json!({"username":owner_profile["handle"]}),
        )
        .await
        .unwrap();

    let second = signed_in(&root.path().join("second"), &base_url, &fixture.owner_token).await;
    assert!(!second.identity().unwrap().enrolled);
    let link = second
        .execute("devices.link.startSelf", Value::Null)
        .await
        .unwrap();
    let code = link
        .events
        .iter()
        .find(|event| event["type"] == "devices.link.selfPending")
        .unwrap()["userCode"]
        .clone();
    owner
        .execute("devices.link.approve", json!({"userCode":code}))
        .await
        .unwrap();
    second.poll_link().await.unwrap();
    assert!(second.identity().unwrap().enrolled);

    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let (requests, mut host_requests) = mpsc::channel(128);
    let (output, output_rx) = broadcast::channel(256);
    let (controls, mut received_controls) = mpsc::channel(128);
    let (bytes, mut byte_requests) = mpsc::channel::<(Vec<u8>, oneshot::Sender<()>)>(128);
    let checkpoint = crate::terminal::Checkpoint::new(
        crate::terminal::TerminalSize::new(24, 80).unwrap(),
        crate::terminal::TerminalScreen::Primary,
        b"{}".to_vec(),
        0,
        0,
        false,
    )
    .unwrap();
    let host_output = output.clone();
    let terminal = tokio::spawn(async move {
        let mut sequence = 0;
        loop {
            tokio::select! {
                request=host_requests.recv()=>match request {
                    Some(HostRequest::Bootstrap {request_id,reply})=>{
                        host_output.send(PublishedFrame::BootstrapBarrier {request_id}).unwrap_or_else(|_|panic!("host output closed"));
                        let _reply=reply.send(Ok(CheckpointCut {checkpoint:checkpoint.clone(),next_sequence:sequence}));
                    }
                    Some(HostRequest::Control {control,authorization,reply,..})=>{
                        if authorization.is_cancelled(){let _reply=reply.send(Err("revoked".into()));continue;}
                        controls.send(control).await.unwrap();
                        let _reply=reply.send(Ok(Value::Null));
                    }
                    Some(HostRequest::Disconnected {..} | HostRequest::Connected {..} | HostRequest::ResetPresence)=>{},
                    None=>break,
                },
                Some((chunk,reply))=byte_requests.recv()=>{
                    host_output.send(PublishedFrame::Raw {sequence,bytes:chunk.into()}).unwrap_or_else(|_|panic!("host output closed"));sequence+=1;
                    let _reply=reply.send(());
                }
            }
        }
    });
    owner
        .publish(
            LocalPublication {
                session_id: id,
                incarnation_id: incarnation,
                name: "encrypted test".into(),
                room_id: None,
                shared_with: BTreeSet::new(),
            },
            requests,
            output_rx,
        )
        .await
        .unwrap();
    ready(&owner, id).await;
    owner.execute("session.share",json!({"requestId":Uuid::now_v7(),"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"userIds":[friend.identity().unwrap().user_id]})).await.unwrap();
    ready(&owner, id).await;
    let mut remote = friend.connect_remote(id).await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(10), remote.updates.recv())
            .await
            .unwrap(),
        Some(RemoteUpdate::Checkpoint {
            next_sequence: 0,
            ..
        })
    ));
    let (sent, seen) = oneshot::channel();
    bytes
        .send((b"visible only at endpoints".to_vec(), sent))
        .await
        .unwrap();
    seen.await.unwrap();
    let update = tokio::time::timeout(Duration::from_secs(10), remote.updates.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(update,RemoteUpdate::Raw {sequence:0,bytes} if bytes.as_ref()==b"visible only at endpoints")
    );
    remote
        .send_control(TerminalControl::Input {
            bytes: b"command".to_vec(),
        })
        .await
        .unwrap();
    assert!(
        matches!(received_controls.recv().await,Some(TerminalControl::Input {bytes}) if bytes==b"command")
    );
    remote.request_checkpoint().await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(10), remote.updates.recv())
            .await
            .unwrap(),
        Some(RemoteUpdate::Checkpoint {
            next_sequence: 1,
            ..
        })
    ));
    let mut own_remote = second.connect_remote(id).await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(10), own_remote.updates.recv())
            .await
            .unwrap(),
        Some(RemoteUpdate::Checkpoint {
            next_sequence: 1,
            ..
        })
    ));
    own_remote
        .send_control(TerminalControl::Interrupt)
        .await
        .unwrap();
    assert!(matches!(
        received_controls.recv().await,
        Some(TerminalControl::Interrupt)
    ));
    let publication = Arc::clone(&owner.inner.publications.lock().await[&id]);
    publication.invalidate().await;
    publication.info.write().await.shared_with.clear();
    *publication.pending_shares.lock().await = Some(BTreeSet::new());
    owner.reconcile_shares().await.unwrap();
    assert!(publication.pending_shares.lock().await.is_none());
    assert!(
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(update) = remote.updates.recv().await {
                if matches!(update, RemoteUpdate::Closed { .. }) {
                    break;
                }
            }
        })
        .await
        .is_ok()
    );
    assert!(
        remote
            .send_control(TerminalControl::Interrupt)
            .await
            .is_err()
    );
    ready(&owner, id).await;
    owner
        .execute(
            "devices.revoke",
            json!({"deviceId":second.identity().unwrap().device_id}),
        )
        .await
        .unwrap();
    assert!(owner.inner.state.lock().await.blocked_devices.is_empty());
    assert!(second.connect_remote(id).await.is_err());
    owner.unpublish(id).await.unwrap();
    owner.shutdown().await;
    friend.shutdown().await;
    second.shutdown().await;
    terminal.abort();
}
