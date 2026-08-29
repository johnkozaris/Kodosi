use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalResizeIdentity {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "expectedRuntimeIncarnationId")]
    pub expected_runtime_incarnation_id: String,
    #[serde(rename = "subscriptionId")]
    pub subscription_id: String,
    #[serde(rename = "subscriptionGeneration")]
    pub subscription_generation: u64,
    #[serde(rename = "surfaceGeneration")]
    pub surface_generation: u64,
    pub cols: u16,
    pub rows: u16,
    #[serde(rename = "widthPixels")]
    pub width_pixels: u32,
    #[serde(rename = "heightPixels")]
    pub height_pixels: u32,
    #[serde(rename = "cellWidthPixels")]
    pub cell_width_pixels: u32,
    #[serde(rename = "cellHeightPixels")]
    pub cell_height_pixels: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TerminalCommand {
    #[serde(rename = "session.inputBytes")]
    InputBytes {
        #[serde(rename = "sessionId")]
        session_id: String,
        bytes: Vec<u8>,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
        #[serde(
            rename = "subscriptionId",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        subscription_id: Option<String>,
        #[serde(
            rename = "subscriptionGeneration",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        subscription_generation: Option<u64>,
    },
    #[serde(rename = "session.resize")]
    Resize {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(flatten)]
        identity: TerminalResizeIdentity,
        claim: bool,
    },
    #[serde(skip)]
    #[specta(skip)]
    HeadlessResize {
        session_id: String,
        cols: u16,
        rows: u16,
        expected_runtime_incarnation_id: String,
        subscription_id: String,
        subscription_generation: u64,
    },
    #[serde(rename = "session.focus")]
    Focus {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
    #[serde(rename = "session.blur")]
    Blur {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(rename = "expectedRuntimeIncarnationId")]
        expected_runtime_incarnation_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    #[serde(rename = "term.bell")]
    Bell {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "term.title")]
    Title {
        #[serde(rename = "sessionId")]
        session_id: String,
        title: Option<String>,
    },
    #[serde(rename = "term.focusApplied")]
    FocusApplied {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "runtimeIncarnationId")]
        runtime_incarnation_id: String,
    },
    #[serde(rename = "term.focusRejected")]
    FocusRejected {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "runtimeIncarnationId")]
        runtime_incarnation_id: String,
        reason: String,
    },
    #[serde(rename = "term.resizeApplied")]
    ResizeApplied {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(flatten)]
        identity: TerminalResizeIdentity,
    },
    #[serde(rename = "term.resizeRejected")]
    ResizeRejected {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(flatten)]
        identity: TerminalResizeIdentity,
        reason: String,
    },
    #[serde(rename = "term.notification")]
    Notification {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
}

impl TerminalEvent {
    pub fn session_id(&self) -> &str {
        match self {
            Self::Bell { session_id }
            | Self::Title { session_id, .. }
            | Self::FocusApplied { session_id, .. }
            | Self::FocusRejected { session_id, .. }
            | Self::ResizeApplied { session_id, .. }
            | Self::ResizeRejected { session_id, .. }
            | Self::Notification { session_id, .. } => session_id,
        }
    }
}
