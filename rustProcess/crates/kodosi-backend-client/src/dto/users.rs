use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileDto {
    pub id: String,
    pub handle: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummaryDto {
    pub id: String,
    pub handle: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}
