use crate::host_protocol::{FriendEntry, FriendRequestEntry, FriendsEvent};
use kodosi_backend_client::api::{
    BackendFriendRequest, BackendFriendRequestInbox, BackendUserSummary,
};

pub(crate) fn friends_snapshot(
    friends: Vec<BackendUserSummary>,
    inbox: BackendFriendRequestInbox,
    request_id: Option<String>,
) -> FriendsEvent {
    FriendsEvent::Snapshot {
        friends: friends.into_iter().map(friend_entry_from_summary).collect(),
        incoming: inbox
            .incoming
            .into_iter()
            .map(friend_request_entry_from_dto)
            .collect(),
        outgoing: inbox
            .outgoing
            .into_iter()
            .map(friend_request_entry_from_dto)
            .collect(),
        request_id,
    }
}

fn friend_entry_from_summary(dto: BackendUserSummary) -> FriendEntry {
    FriendEntry {
        user_id: dto.id,
        handle: dto.handle,
        display_name: dto.display_name,
        avatar_url: dto.avatar_url,
    }
}

fn friend_request_entry_from_dto(dto: BackendFriendRequest) -> FriendRequestEntry {
    FriendRequestEntry {
        user_id: dto.user_id,
        handle: dto.handle,
        display_name: dto.display_name,
        avatar_url: dto.avatar_url,
        created_at: dto.created_at,
    }
}
