use super::shared_session_registry::SharedRoom;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedRoom {
    pub(crate) id: String,
    pub(crate) name: String,
}

impl From<&SelectedRoom> for SharedRoom {
    fn from(value: &SelectedRoom) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
        }
    }
}

impl From<&SharedRoom> for SelectedRoom {
    fn from(value: &SharedRoom) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKeyDistributionResult {
    Distributed,
    PendingRecipients,
}
