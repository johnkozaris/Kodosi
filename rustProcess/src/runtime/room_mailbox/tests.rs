use super::{
    ChatSyncProgress, RoomMailboxTarget, chat_rooms_remaining_after_pass, next_room_after,
    process_chat_page, process_task_batch, retain_scanned_task_cursors, room_chat_targets,
    rotating_room_batch, task_delivery_targets,
};
use crate::{
    rooms::mailbox_store::{
        MailboxDestination, RoomDeliveryState, RoomTaskDeliveryCursor, RoomTaskDeliveryReason,
    },
    runtime::rooms::{MailboxRoomChatMessage, MailboxRoomChatPage, MailboxRoomTask},
};
use kodosi_backend_client::api::{BackendRoomChatMessage, BackendRoomTask};
use kodosi_domain::ids::{SessionId, UserId};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use time::OffsetDateTime;

fn user(id: u128) -> UserId {
    UserId::try_from(format!("{id:032x}").as_str()).expect("uuid")
}

fn room_target(session_id: SessionId) -> RoomMailboxTarget {
    RoomMailboxTarget {
        mailbox: MailboxDestination::new(session_id, uuid::Uuid::now_v7()),
        backend_incarnation_id: uuid::Uuid::now_v7(),
    }
}

#[test]
fn broadcast_room_chat_reaches_other_agents_attached_to_that_room() {
    let author = SessionId::new();
    let teammate = SessionId::new();
    let other_room = SessionId::new();
    let author_target = room_target(author);
    let teammate_target = room_target(teammate);
    let by_room = HashMap::from([
        (
            "room-a".to_owned(),
            BTreeMap::from([(author, author_target), (teammate, teammate_target)]),
        ),
        (
            "room-b".to_owned(),
            BTreeMap::from([(other_room, room_target(other_room))]),
        ),
    ]);

    assert_eq!(
        room_chat_targets("room-a", Some(author), &[], &[], &by_room),
        BTreeSet::from([teammate_target.mailbox])
    );
    assert!(room_chat_targets("unknown", None, &[], &[], &by_room).is_empty());
}

#[test]
fn directed_room_chat_reaches_only_named_local_room_sessions() {
    let author = SessionId::new();
    let first = SessionId::new();
    let second = SessionId::new();
    let unlisted = SessionId::new();
    let first_target = room_target(first);
    let second_target = room_target(second);
    let by_room = HashMap::from([(
        "room-a".to_owned(),
        BTreeMap::from([
            (author, room_target(author)),
            (first, first_target),
            (second, second_target),
            (unlisted, room_target(unlisted)),
        ]),
    )]);

    assert_eq!(
        room_chat_targets("room-a", Some(author), &[first], &[], &by_room),
        BTreeSet::from([first_target.mailbox])
    );
    assert_eq!(
        room_chat_targets("room-a", Some(author), &[first, second], &[], &by_room),
        BTreeSet::from([first_target.mailbox, second_target.mailbox])
    );
}

#[test]
fn human_only_directed_room_chat_reaches_no_agent_mailbox() {
    let agent = SessionId::new();
    let by_room = HashMap::from([(
        "room-a".to_owned(),
        BTreeMap::from([(agent, room_target(agent))]),
    )]);

    assert!(room_chat_targets("room-a", None, &[], &[user(1)], &by_room).is_empty());
}

#[test]
fn directed_room_chat_excludes_author_cross_room_and_nonlocal_targets() {
    let author = SessionId::new();
    let teammate = SessionId::new();
    let other_room = SessionId::new();
    let nonlocal = SessionId::new();
    let teammate_target = room_target(teammate);
    let by_room = HashMap::from([
        (
            "room-a".to_owned(),
            BTreeMap::from([(author, room_target(author)), (teammate, teammate_target)]),
        ),
        (
            "room-b".to_owned(),
            BTreeMap::from([(other_room, room_target(other_room))]),
        ),
    ]);

    assert_eq!(
        room_chat_targets(
            "room-a",
            Some(author),
            &[author, teammate, other_room, nonlocal],
            &[],
            &by_room,
        ),
        BTreeSet::from([teammate_target.mailbox])
    );
}

#[test]
fn task_reassignment_notifies_previous_and_current_local_agents_once() {
    let previous = SessionId::new();
    let current = SessionId::new();
    let unrelated = SessionId::new();
    let previous_target = room_target(previous);
    let current_target = room_target(current);
    let local = BTreeMap::from([
        (previous, previous_target),
        (current, current_target),
        (unrelated, room_target(unrelated)),
    ]);
    let previous_cursor = RoomTaskDeliveryCursor {
        fingerprint: "prior".to_owned(),
        assigned_session_id: Some(previous),
        assigned_session_incarnation_id: Some(previous_target.backend_incarnation_id),
        mailbox_destination: Some(previous_target.mailbox),
    };

    assert_eq!(
        task_delivery_targets(
            Some((current, current_target.backend_incarnation_id)),
            Some(&previous_cursor),
            &local,
        ),
        BTreeMap::from([
            (
                previous_target.mailbox,
                RoomTaskDeliveryReason::ReassignedPreviousAssignee,
            ),
            (
                current_target.mailbox,
                RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            ),
        ])
    );
}

#[test]
fn task_delivery_rejects_stale_assignment_incarnation() {
    let session_id = SessionId::new();
    let current = room_target(session_id);
    let stale_incarnation = uuid::Uuid::now_v7();

    assert_ne!(stale_incarnation, current.backend_incarnation_id);
    assert!(
        task_delivery_targets(
            Some((session_id, stale_incarnation)),
            None,
            &BTreeMap::from([(session_id, current)]),
        )
        .is_empty()
    );
}

#[test]
fn reassignment_to_nonlocal_session_still_notifies_previous_local_agent() {
    let previous = SessionId::new();
    let nonlocal = SessionId::new();
    let previous_target = room_target(previous);
    let previous_cursor = RoomTaskDeliveryCursor {
        fingerprint: "prior".to_owned(),
        assigned_session_id: Some(previous),
        assigned_session_incarnation_id: Some(previous_target.backend_incarnation_id),
        mailbox_destination: Some(previous_target.mailbox),
    };

    assert_eq!(
        task_delivery_targets(
            Some((nonlocal, uuid::Uuid::now_v7())),
            Some(&previous_cursor),
            &BTreeMap::from([(previous, previous_target)]),
        ),
        BTreeMap::from([(
            previous_target.mailbox,
            RoomTaskDeliveryReason::ReassignedPreviousAssignee,
        )])
    );
}

#[test]
fn task_unassignment_notifies_previous_local_agent_with_non_actionable_reason() {
    let previous = SessionId::new();
    let previous_target = room_target(previous);
    let previous_cursor = RoomTaskDeliveryCursor {
        fingerprint: "prior".to_owned(),
        assigned_session_id: Some(previous),
        assigned_session_incarnation_id: Some(previous_target.backend_incarnation_id),
        mailbox_destination: Some(previous_target.mailbox),
    };

    assert_eq!(
        task_delivery_targets(
            None,
            Some(&previous_cursor),
            &BTreeMap::from([(previous, previous_target)]),
        ),
        BTreeMap::from([(
            previous_target.mailbox,
            RoomTaskDeliveryReason::UnassignedPreviousAssignee,
        )])
    );
}

#[test]
fn task_delivery_targets_current_assignment_incarnation() {
    let session_id = SessionId::new();
    let current = room_target(session_id);

    assert_eq!(
        task_delivery_targets(
            Some((session_id, current.backend_incarnation_id)),
            None,
            &BTreeMap::from([(session_id, current)]),
        ),
        BTreeMap::from([(
            current.mailbox,
            RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
        )])
    );
}

#[test]
fn room_batches_rotate_and_cover_rooms_beyond_first_page() {
    let room_ids = (0..300)
        .map(|index| format!("room-{index:03}"))
        .collect::<Vec<_>>();

    let (first, next) = rotating_room_batch(&room_ids, None, 256);
    let (second, _) = rotating_room_batch(&room_ids, next.as_deref(), room_ids.len() - first.len());

    assert_eq!(first.len(), 256);
    assert_eq!(next.as_deref(), Some("room-256"));
    assert!(second.iter().any(|room_id| room_id == "room-299"));
    assert_eq!(
        first.iter().chain(&second).collect::<BTreeSet<_>>().len(),
        300
    );
}

#[test]
fn room_batch_marker_wraps_when_room_was_removed() {
    let room_ids = vec!["a".to_owned(), "c".to_owned(), "e".to_owned()];

    let (selected, _) = rotating_room_batch(&room_ids, Some("d"), 2);

    assert_eq!(selected, ["e".to_owned(), "a".to_owned()]);
}

#[test]
fn capped_hot_room_advances_to_next_room_instead_of_starving_it() {
    let room_ids = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];

    assert_eq!(next_room_after(&room_ids, "a").as_deref(), Some("b"));
    assert_eq!(next_room_after(&room_ids, "c").as_deref(), Some("a"));
}

#[test]
fn capped_pass_preserves_the_unfinished_room_sweep() {
    let progress = ChatSyncProgress {
        completed_rooms: 2,
        last_visited_room_id: Some("hot".to_owned()),
        hit_delivery_limit: true,
    };

    assert_eq!(chat_rooms_remaining_after_pass(44, 300, &progress), 300);
}

#[test]
fn task_cursor_retention_preserves_unscanned_room_batches() {
    let mut delivery = RoomDeliveryState::default();
    for index in 0..300 {
        delivery.task_cursors.insert(
            format!("room-{index:03}/task"),
            RoomTaskDeliveryCursor {
                fingerprint: "unchanged".to_owned(),
                assigned_session_id: None,
                assigned_session_incarnation_id: None,
                mailbox_destination: None,
            },
        );
    }
    let first_rooms = (0..256)
        .map(|index| format!("room-{index:03}"))
        .collect::<Vec<_>>();
    let first_scanned = first_rooms.iter().map(String::as_str).collect();
    let first_seen = first_rooms
        .iter()
        .map(|room| format!("{room}/task"))
        .collect();

    retain_scanned_task_cursors(&mut delivery, &first_scanned, &first_seen);

    let second_rooms = (256..300)
        .map(|index| format!("room-{index:03}"))
        .collect::<Vec<_>>();
    let second_scanned = second_rooms.iter().map(String::as_str).collect();
    let second_seen = second_rooms
        .iter()
        .map(|room| format!("{room}/task"))
        .collect();
    retain_scanned_task_cursors(&mut delivery, &second_scanned, &second_seen);

    assert_eq!(delivery.task_cursors.len(), 300);
}

#[test]
fn terminally_malformed_chat_advances_before_delivering_next_message() {
    let malformed = MailboxRoomChatMessage::Quarantined {
        id: "message-7".to_owned(),
        room_id: "room-a".to_owned(),
        seq: 7,
        reason: "invalid encrypted payload".to_owned(),
    };
    let valid = MailboxRoomChatMessage::Decrypted(BackendRoomChatMessage {
        id: "message-8".to_owned(),
        room_id: "room-a".to_owned(),
        author_user_id: user(1).to_string(),
        author_session_id: None,
        author_kind: "Human".to_owned(),
        body: "valid".to_owned(),
        recipient_session_ids: Vec::new(),
        recipient_user_ids: Vec::new(),
        seq: 8,
        posted_at: OffsetDateTime::UNIX_EPOCH,
    });
    let mut delivered = Vec::new();
    let mut quarantined = Vec::new();
    let mut commits = Vec::new();

    let progress = process_chat_page(
        MailboxRoomChatPage {
            items: vec![valid, malformed],
            has_more: false,
            next_since: None,
        },
        6,
        |message| {
            delivered.push(message.seq);
            Ok(())
        },
        |_, _, seq, _| quarantined.push(seq),
        |seq| {
            commits.push(seq);
            Ok(())
        },
    )
    .expect("page should continue after terminal quarantine");

    assert_eq!(quarantined, [7]);
    assert_eq!(delivered, [8]);
    assert_eq!(commits, [7, 8]);
    assert_eq!(progress.cursor, 8);
}

#[test]
fn mailbox_continues_across_short_pages_when_metadata_has_more() {
    let first = mailbox_chat("message-7", 7);
    let second = mailbox_chat("message-8", 8);
    let mut delivered = Vec::new();
    let mut cursor = 6;

    for page in [
        MailboxRoomChatPage {
            items: vec![first],
            has_more: true,
            next_since: Some(7),
        },
        MailboxRoomChatPage {
            items: vec![second],
            has_more: false,
            next_since: None,
        },
    ] {
        let progress = process_chat_page(
            page,
            cursor,
            |message| {
                delivered.push(message.seq);
                Ok(())
            },
            |_, _, _, _| {},
            |_| Ok(()),
        )
        .expect("mailbox page");
        cursor = progress.cursor;
        if !progress.has_more {
            break;
        }
    }

    assert_eq!(delivered, [7, 8]);
    assert_eq!(cursor, 8);
}

#[test]
fn poison_task_does_not_block_later_tasks_or_rooms() {
    let malformed = MailboxRoomTask::Quarantined {
        id: "task-1".to_owned(),
        room_id: "room-a".to_owned(),
        updated_at: OffsetDateTime::UNIX_EPOCH,
        reason: "invalid encrypted payload".to_owned(),
    };
    let valid_in_same_room = mailbox_task("task-2", "room-a", 1);
    let valid_in_next_room = mailbox_task("task-3", "room-b", 2);
    let mut delivered = Vec::new();
    let mut quarantined = Vec::new();

    process_task_batch(
        vec![valid_in_same_room, malformed],
        |task| {
            delivered.push((task.room_id, task.id));
            Ok(())
        },
        |id, room_id, _, _| quarantined.push((room_id.to_owned(), id.to_owned())),
    )
    .expect("poison task should be quarantined");
    process_task_batch(
        vec![valid_in_next_room],
        |task| {
            delivered.push((task.room_id, task.id));
            Ok(())
        },
        |id, room_id, _, _| quarantined.push((room_id.to_owned(), id.to_owned())),
    )
    .expect("next room should still be processed");

    assert_eq!(quarantined, [("room-a".to_owned(), "task-1".to_owned())]);
    assert_eq!(
        delivered,
        [
            ("room-a".to_owned(), "task-2".to_owned()),
            ("room-b".to_owned(), "task-3".to_owned())
        ]
    );
}

fn mailbox_task(id: &str, room_id: &str, updated_second: i64) -> MailboxRoomTask {
    MailboxRoomTask::Decrypted(Box::new(BackendRoomTask {
        id: id.to_owned(),
        room_id: room_id.to_owned(),
        created_by_user_id: user(1).to_string(),
        title: "valid".to_owned(),
        description: None,
        status: "open".to_owned(),
        revision: 0,
        assigned_session_id: None,
        assigned_session_incarnation_id: None,
        due_at: None,
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::from_unix_timestamp(updated_second).expect("test timestamp"),
        completed_at: None,
        result: None,
        result_author_user_id: None,
    }))
}

fn mailbox_chat(id: &str, seq: i64) -> MailboxRoomChatMessage {
    MailboxRoomChatMessage::Decrypted(BackendRoomChatMessage {
        id: id.to_owned(),
        room_id: "room-a".to_owned(),
        author_user_id: user(1).to_string(),
        author_session_id: None,
        author_kind: "Human".to_owned(),
        body: "valid".to_owned(),
        recipient_session_ids: Vec::new(),
        recipient_user_ids: Vec::new(),
        seq,
        posted_at: OffsetDateTime::UNIX_EPOCH,
    })
}
