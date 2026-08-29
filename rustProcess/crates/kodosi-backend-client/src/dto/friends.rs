use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequestDto {
    pub user_id: String,
    pub handle: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequestInboxDto {
    #[serde(default)]
    pub incoming: Vec<FriendRequestDto>,
    #[serde(default)]
    pub outgoing: Vec<FriendRequestDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FriendRequestHandleRequest<'a> {
    pub username: &'a str,
}
