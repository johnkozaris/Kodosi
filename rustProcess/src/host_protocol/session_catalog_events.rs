use super::{RoomListEntry, SessionEvent, SessionListEntry};

pub(crate) fn session_catalog_snapshot_event(sessions: Vec<SessionListEntry>) -> SessionEvent {
    SessionEvent::List { sessions }
}

pub(crate) fn room_catalog_event(rooms: &[RoomListEntry]) -> SessionEvent {
    SessionEvent::RoomList {
        rooms: rooms.to_vec(),
    }
}
