use kodosi_backend_client::{
    api::{
        BackendSessionDetail as BackendSessionResponse, CreateBackendSessionRequest,
        UpdateBackendSessionRequest,
    },
    labels::BackendToolKind,
};
use kodosi_domain::{
    permissions::{AccessLevel, DefaultAudienceAccess, ShareScope},
    session::SessionState,
};
use uuid::Uuid;

use super::scope::SelectedRoom;

pub(crate) struct BackendSessionDetail {
    pub(crate) id: String,
    pub(crate) incarnation_id: Uuid,
    pub(crate) title: String,
    pub(crate) scope: ShareScope,
    pub(crate) room_id: Option<String>,
    pub(crate) default_access: AccessLevel,
    pub(crate) effective_access: Option<AccessLevel>,
    pub(crate) status: SessionState,
}

impl From<BackendSessionResponse> for BackendSessionDetail {
    fn from(value: BackendSessionResponse) -> Self {
        Self {
            id: value.id,
            incarnation_id: value.incarnation_id,
            title: value.title,
            scope: value.scope,
            room_id: value.room_id,
            default_access: value.default_access,
            effective_access: value.effective_access,
            status: value.status,
        }
    }
}

pub(crate) fn create_session_request(
    id: String,
    idempotency_key: Uuid,
    title: String,
    current_access: AccessLevel,
    scope: ShareScope,
    owner_secret: impl Into<zeroize::Zeroizing<String>>,
    room: Option<&SelectedRoom>,
) -> CreateBackendSessionRequest {
    CreateBackendSessionRequest {
        id,
        idempotency_key,
        title,
        scope,
        tool_kind: BackendToolKind::Generic,
        default_access: DefaultAudienceAccess::clamp(current_access),
        owner_secret: owner_secret.into(),
        room_id: room.map(|selected| selected.id.clone()),
    }
}

pub(crate) fn title_update_request(
    title: &str,
    expected_incarnation_id: Uuid,
) -> UpdateBackendSessionRequest {
    UpdateBackendSessionRequest {
        expected_incarnation_id,
        title: Some(title.to_owned()),
        scope: None,
        default_access: None,
        room_id: None,
    }
}

pub(crate) fn scope_update_request_for_room_id(
    scope: ShareScope,
    room_id: Option<&str>,
    expected_incarnation_id: Uuid,
) -> UpdateBackendSessionRequest {
    UpdateBackendSessionRequest {
        expected_incarnation_id,
        title: None,
        scope: Some(scope),
        default_access: None,
        room_id: room_id.map(ToOwned::to_owned),
    }
}
