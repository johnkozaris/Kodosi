use super::{
    DurableRoomMutation, RoomChatCollector, is_conflict, is_terminal_room_chat_content_error,
    is_terminal_room_task_content_error, room_mutation_fingerprint_error, same_chat_create,
    same_task_create,
};
use crate::AppError;
use kodosi_backend_client::api::{BackendRoomChatMessage, BackendRoomChatPage, BackendRoomTask};
use time::OffsetDateTime;

#[test]
fn decrypted_create_fingerprints_ignore_ciphertext_randomness_but_reject_mismatch() {
    let sessions = vec!["session-b".to_owned(), "session-a".to_owned()];
    let users = vec!["user-b".to_owned(), "user-a".to_owned()];
    let message = BackendRoomChatMessage {
        id: "message-1".to_owned(),
        room_id: "room-1".to_owned(),
        author_user_id: "author".to_owned(),
        author_session_id: None,
        author_kind: "Human".to_owned(),
        body: "same plaintext".to_owned(),
        recipient_session_ids: vec!["session-a".to_owned(), "session-b".to_owned()],
        recipient_user_ids: vec!["user-a".to_owned(), "user-b".to_owned()],
        seq: 1,
        posted_at: OffsetDateTime::UNIX_EPOCH,
    };
    assert!(same_chat_create(
        &message,
        "room-1",
        "same plaintext",
        None,
        "Human",
        &sessions,
        &users,
    ));
    assert!(!same_chat_create(
        &message,
        "room-1",
        "changed plaintext",
        None,
        "Human",
        &sessions,
        &users,
    ));

    let due_at = OffsetDateTime::from_unix_timestamp(42).expect("timestamp");
    let task = BackendRoomTask {
        id: "task-1".to_owned(),
        room_id: "room-1".to_owned(),
        created_by_user_id: "author".to_owned(),
        title: "same title".to_owned(),
        description: Some("same description".to_owned()),
        status: "Open".to_owned(),
        revision: 0,
        assigned_session_id: Some("session-a".to_owned()),
        assigned_session_incarnation_id: None,
        due_at: Some(due_at),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
        completed_at: None,
        result: None,
        result_author_user_id: None,
        content_unavailable: false,
    };
    assert!(same_task_create(
        &task,
        "room-1",
        "same title",
        Some("same description"),
        Some("session-a"),
        None,
        Some(due_at),
    ));
    assert!(!same_task_create(
        &task,
        "room-1",
        "changed title",
        Some("same description"),
        Some("session-a"),
        None,
        Some(due_at),
    ));
}

#[test]
fn conflict_detection_uses_typed_http_status() {
    let conflict = kodosi_backend_client::BackendClientError::HttpProblem {
        status: 409,
        code: Some("conflict".to_owned()),
        detail: "different ciphertext".to_owned(),
    };
    let validation = kodosi_backend_client::BackendClientError::HttpProblem {
        status: 400,
        code: None,
        detail: "bad request".to_owned(),
    };

    assert!(is_conflict(&conflict));
    assert!(!is_conflict(&validation));
    assert!(
        room_mutation_fingerprint_error("room task")
            .to_string()
            .contains("different request fingerprint")
    );
}

#[test]
fn committed_projection_failure_warns_cli_not_to_retry() {
    let result = DurableRoomMutation::<()> {
        entity_id: "entity-1".to_owned(),
        projection: Err(AppError::Unsupported {
            reason: "decrypt failed".to_owned(),
        }),
    }
    .into_cli_result()
    .expect_err("projection failure should be explicit");

    assert!(result.to_string().contains("committed as entity-1"));
    assert!(result.to_string().contains("do not retry"));
}

#[test]
fn malformed_room_chat_content_is_terminal_but_transport_failure_is_not() {
    assert!(is_terminal_room_chat_content_error(
        &AppError::InvalidBackendData {
            field: "room.encryptedContent".to_owned(),
            reason: "invalid encrypted-content envelope".to_owned(),
        }
    ));
    assert!(!is_terminal_room_chat_content_error(&AppError::Io(
        std::io::Error::new(std::io::ErrorKind::TimedOut, "temporary")
    )));
}

#[test]
fn malformed_room_task_content_is_terminal_but_key_availability_is_not() {
    assert!(is_terminal_room_task_content_error(
        &AppError::InvalidBackendData {
            field: "roomTask.result".to_owned(),
            reason: "missing signer attribution".to_owned(),
        }
    ));
    assert!(!is_terminal_room_task_content_error(&AppError::NotFound));
    assert!(!is_terminal_room_task_content_error(
        &AppError::Unauthorized
    ));
}

#[test]
fn room_chat_snapshot_collects_all_byte_bounded_pages() {
    let mut collector = RoomChatCollector::new(10, true);

    assert_eq!(
        collector
            .accept(BackendRoomChatPage {
                items: vec![chat_message("message-1", 1)],
                has_more: true,
                next_since: Some(1),
                next_before: None,
            })
            .expect("first snapshot page"),
        Some(1)
    );
    assert_eq!(
        collector
            .accept(BackendRoomChatPage {
                items: vec![chat_message("message-2", 2)],
                has_more: false,
                next_since: None,
                next_before: None,
            })
            .expect("terminal snapshot page"),
        None
    );

    assert_eq!(
        collector
            .into_items()
            .into_iter()
            .map(|message| message.seq)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn unavailable_history_preserves_message_identity_and_readable_page() {
    let mut unavailable = chat_message("older", 1);
    super::present_chat_decryption(&mut unavailable, Err(AppError::RoomContentNotRecipient))
        .unwrap();
    assert_eq!(unavailable.id, "older");
    assert_eq!(unavailable.seq, 1);
    assert_eq!(
        unavailable.body,
        "This message was sent before this device had access."
    );
    let readable = chat_message("newer", 2);
    let mut collector = RoomChatCollector::new(10, true);
    collector
        .accept(BackendRoomChatPage {
            items: vec![unavailable, readable],
            has_more: false,
            next_since: None,
            next_before: None,
        })
        .unwrap();
    assert_eq!(collector.into_items().len(), 2);
    let mut invalid = chat_message("tampered", 3);
    assert!(
        super::present_chat_decryption(
            &mut invalid,
            Err(AppError::InvalidBackendData {
                field: "room.encryptedContent.signature".to_owned(),
                reason: "invalid signature".to_owned(),
            })
        )
        .is_err()
    );
    assert!(is_terminal_room_chat_content_error(
        &AppError::RoomContentNotRecipient
    ));
}

#[tokio::test]
async fn task_snapshot_conflict_discards_partial_pages_before_retry() {
    use axum::{Json, extract::Query, response::IntoResponse};
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    let requests = Arc::new(Mutex::new(Vec::<HashMap<String, String>>::new()));
    let router = axum::Router::new().route("/api/rooms/room/tasks", axum::routing::get({
        let requests = Arc::clone(&requests);
        move |Query(query): Query<HashMap<String, String>>| {
            let requests = Arc::clone(&requests);
            async move {
                let index = {
                    let mut requests = requests.lock().unwrap();
                    requests.push(query);
                    requests.len()
                };
                match index {
                    1 => ([("Kodosi-Has-More", "true"), ("Kodosi-Next-Offset", "1"),
                        ("Kodosi-Task-Snapshot", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")],
                        Json(serde_json::json!([{
                            "id":"old", "roomId":"room", "createdByUserId":"owner", "title":"unpublished ciphertext",
                            "description":null, "status":"Open", "revision":1, "assignedSessionId":null,
                            "assignedSessionIncarnationId":null, "dueAt":null, "createdAt":"2026-09-01T00:00:00Z",
                            "updatedAt":"2026-09-01T00:00:00Z", "completedAt":null, "result":null, "resultAuthorUserId":null
                        }]))).into_response(),
                    2 => (axum::http::StatusCode::CONFLICT, Json(serde_json::json!({
                        "code":"CONCURRENT_MODIFICATION", "detail":"task order changed"
                    }))).into_response(),
                    _ => ([("Kodosi-Has-More", "false"),
                        ("Kodosi-Task-Snapshot", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")],
                        Json(serde_json::json!([]))).into_response(),
                }
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut config = crate::config::AppConfig::default();
    config.backend.api = Some(format!("http://{address}/"));
    let dependencies = crate::runtime::RuntimeDependencies::isolated(&config).unwrap();
    let runtime = crate::runtime::Runtime::with_dependencies(
        config,
        tokio_util::sync::CancellationToken::new(),
        dependencies,
    )
    .unwrap();
    let tasks = super::RoomApplication::new(&runtime)
        .fetch_room_tasks("room", None, None)
        .await
        .unwrap();
    assert!(
        tasks.is_empty(),
        "no record from an invalidated snapshot may be published"
    );
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].get("offset").map(String::as_str), Some("0"));
    assert_eq!(requests[1].get("snapshot"), Some(&"a".repeat(64)));
    assert_eq!(requests[1].get("offset").map(String::as_str), Some("1"));
    assert_eq!(requests[2].get("offset").map(String::as_str), Some("0"));
    assert!(!requests[2].contains_key("snapshot"));
    server.abort();
}

fn chat_message(id: &str, seq: i64) -> BackendRoomChatMessage {
    BackendRoomChatMessage {
        id: id.to_owned(),
        room_id: "room-1".to_owned(),
        author_user_id: "user-1".to_owned(),
        author_session_id: None,
        author_kind: "Human".to_owned(),
        body: "body".to_owned(),
        recipient_session_ids: Vec::new(),
        recipient_user_ids: Vec::new(),
        seq,
        posted_at: OffsetDateTime::UNIX_EPOCH,
    }
}
