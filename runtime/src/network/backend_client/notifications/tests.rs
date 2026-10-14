use super::*;
use crate::{
    identity::keys::DeviceKeys,
    network::{
        BackendConfig,
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
        Some(Scope::Rooms(BTreeMap::from([
            (first, Some(9)),
            (second, Some(2))
        ])))
    );
    waiting.add(&room(second, None)).unwrap();
    assert_eq!(
        waiting.rooms,
        Some(Scope::Rooms(BTreeMap::from([
            (first, Some(9)),
            (second, None)
        ])))
    );
    waiting.add(&Change::of("rooms")).unwrap();
    waiting.add(&room(first, Some(12))).unwrap();
    assert_eq!(waiting.rooms, Some(Scope::All));
    waiting
        .add(&Change {
            surface: "missions".into(),
            room: Some(first),
            sequence: None,
        })
        .unwrap();
    assert_eq!(
        waiting.missions,
        Some(Scope::Rooms(BTreeMap::from([(first, None)])))
    );
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
            json!({"message": "fixture room is no longer available"}),
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
