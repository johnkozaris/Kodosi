use super::*;
use crate::{
    identity::{
        friends::{FriendList, Head, broken},
        keys::DeviceKeys,
    },
    network::{
        BackendConfig,
        backend_client::tests::{compatible_server, scripted_backend, signed_in_at},
        wire::{Change, Notice, TerminalNotice},
    },
};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use uuid::Uuid;
use zeroize::Zeroizing;

#[test]
fn notices_for_one_room_collapse_into_one_read_up_to_the_highest_sequence() {
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
    let room = |room, sequence| Change {
        surface: "rooms".into(),
        room: Some(room),
        sequence,
    };
    let mut waiting = Waiting::default();
    for change in [
        room(first, Some(4)),
        room(first, Some(9)),
        room(first, Some(7)),
    ] {
        waiting.add(&change).unwrap();
    }
    waiting.add(&room(second, Some(2))).unwrap();
    assert_eq!(
        waiting.rooms,
        BTreeMap::from([(first, Some(9)), (second, Some(2))])
    );
    waiting.add(&room(second, None)).unwrap();
    assert_eq!(
        waiting.rooms,
        BTreeMap::from([(first, Some(9)), (second, None)])
    );
    assert!(waiting.add(&Change::of("rooms")).is_err());
    waiting
        .add(&Change {
            surface: "missions".into(),
            room: Some(first),
            sequence: None,
        })
        .unwrap();
    assert_eq!(
        waiting.missions,
        Some(Scope::Rooms(BTreeSet::from([first])))
    );
    waiting.add(&Change::of("missions")).unwrap();
    assert_eq!(waiting.missions, Some(Scope::All));
    assert!(waiting.add(&Change::of("unknown")).is_err());
}

#[test]
fn a_list_refresh_replaces_the_terminal_notices_that_came_before_it() {
    let notice = |session, online| {
        Notice::Terminal(TerminalNotice {
            session,
            incarnation: Uuid::nil(),
            online,
            status: None,
        })
    };
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
    let mut waiting = Waiting::default();
    for each in [
        notice(first, true),
        notice(second, true),
        notice(first, false),
    ] {
        waiting.take(each).unwrap();
    }
    assert_eq!(waiting.terminals.len(), 2);
    assert!(!waiting.terminals[&first].online);
    waiting
        .take(Notice::Changed(Change::of("sessions")))
        .unwrap();
    waiting.take(notice(second, false)).unwrap();
    assert!(waiting.terminals.is_empty());
    assert!(!waiting.is_empty());
}

#[tokio::test]
async fn room_notifications_use_the_renewed_token_on_the_existing_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let room = Uuid::now_v7();
    let server = tokio::spawn(async move {
        let replies = [
            crate::network::http::current_versions(),
            json!({"challengeId": Uuid::now_v7(), "challengeBytes": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}),
            json!({"session": "fixture-session"}),
            json!({"missions": [{"id": room}], "invitations": []}),
            json!({}),
            json!({"error": "fixture room is no longer available"}),
        ];
        let mut room_request = String::new();
        for (index, body) in replies.into_iter().enumerate() {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                headers.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            reader.read_exact(&mut vec![0; length]).await.unwrap();
            if index == 5 {
                room_request = headers;
            }
            let status = if index == 5 {
                "403 Forbidden"
            } else {
                "200 OK"
            };
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            reader
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .unwrap();
            reader.get_mut().shutdown().await.unwrap();
        }
        room_request
    });
    let storage = tempfile::tempdir().unwrap();
    let network = BackendClient::new(BackendConfig {
        api_url: format!("http://{address}/").parse().unwrap(),
        issuer: format!("http://{address}"),
        client_id: "test".into(),
        scopes: vec![],
        data_root: storage.path().to_owned(),
        secret_service: "test".into(),
        isolated: true,
    })
    .unwrap();
    let user = Uuid::now_v7().to_string();
    let admitted = {
        let state = network.inner.state.lock().await;
        Credentials {
            user_id: user.clone(),
            display_name: "You".to_owned(),
            token: Zeroizing::new("expired-token".into()),
            keys: Arc::new(DeviceKeys::load_or_create(&state.secrets, &user).unwrap()),
            enrolled: true,
            notice: None,
            generation: network.generation(),
            cancel: state.account_cancel.clone(),
        }
    };
    let mut renewed = admitted.clone();
    renewed.token = Zeroizing::new("renewed-token".into());
    *network.inner.credentials.write().unwrap() = Some(renewed);
    let mut waiting = Waiting::default();
    waiting.add(&Change::of("missions")).unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        network.refresh_waiting(&admitted, &mut waiting),
    )
    .await
    .unwrap()
    .unwrap();
    let request = server.await.unwrap().to_ascii_lowercase();
    assert!(request.starts_with(&format!("get /api/missions/{room}/keys?")));
    assert!(request.contains("authorization: bearer renewed-token\r\n"));
    assert!(!request.contains("expired-token"));
}

#[tokio::test]
async fn a_friend_list_that_fails_its_check_is_told_and_the_other_changes_still_arrive() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (_root, network, user) = signed_in_at(&address).await;
    let credentials = network.credentials().unwrap();
    let key = credentials.keys.signing_key().unwrap();
    let mut list = FriendList::new(&user);
    let (_, first) = list
        .sign(&Head::default(), &credentials.keys.device_id, &key)
        .unwrap();
    let (kept, _) = list
        .sign(&first, &credentials.keys.device_id, &key)
        .unwrap();
    network
        .inner
        .state
        .lock()
        .await
        .friend_lists
        .put(&user, kept)
        .unwrap();
    let server = scripted_backend(
        listener,
        vec![
            compatible_server(),
            (
                200,
                json!({"challengeId": Uuid::now_v7(), "challengeBytes": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}),
            ),
            (200, json!({"session": "fixture-session"})),
            (200, json!([])),
            (200, json!({"incoming": [], "outgoing": []})),
            (200, json!([])),
            (200, json!({"revision": 1, "lists": []})),
            (200, json!([])),
        ],
    );
    let mut events = network.events();
    let mut waiting = Waiting::default();
    for surface in ["friends", "sessions"] {
        waiting.add(&Change::of(surface)).unwrap();
    }
    tokio::time::timeout(
        Duration::from_secs(5),
        network.refresh_waiting(&credentials, &mut waiting),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(waiting.is_empty());
    let requests = server.await.unwrap();
    assert_eq!(requests.last().unwrap(), "GET /api/sessions HTTP/1.1");
    let told = std::iter::from_fn(|| events.try_recv().ok())
        .map(|sent| sent.event)
        .collect::<Vec<_>>();
    assert_eq!(
        told.iter()
            .map(|event| event["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["system.error", "network.sessions"]
    );
    assert_eq!(told[0]["message"], json!(broken().text()));
}
