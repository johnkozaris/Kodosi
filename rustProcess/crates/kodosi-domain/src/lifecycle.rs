use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Offline,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RemoteSessionAccessState {
    RegisteringDevice,
    AwaitingKey,
    Ready,
    AccessDenied,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RemoteSessionAccessIssue {
    PeerIdentityChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteActionStatus {
    Accepted,
    Duplicate,
    Busy,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    UserRequested,
    ProcessExited,
    Failed,

    Cancelled,
}
