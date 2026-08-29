use std::io;

use kodosi_backend_client::BackendClientError;
use thiserror::Error;

use kodosi_domain::{ids::SessionIdParseError, terminal::TerminalSizeError};

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to build configuration")]
    ConfigSource(#[source] config::ConfigError),
    #[error("failed to deserialize configuration")]
    ConfigDeserialize(#[source] config::ConfigError),
    #[error("I/O failure")]
    Io(#[from] io::Error),
    #[error("HTTP request failed")]
    Http(#[source] reqwest::Error),
    #[error("backend rejected request ({status}{}): {detail}", .code.as_deref().map(|c| format!(", code={c}")).unwrap_or_default())]
    HttpProblem {
        status: u16,
        code: Option<String>,
        detail: String,
    },
    #[error("websocket request failed")]
    WebSocket(#[source] tokio_tungstenite::tungstenite::Error),
    #[error("JSON processing failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("credential store operation failed: {reason}")]
    Keychain { reason: String },
    #[error("backend rejected the current credentials")]
    Unauthorized,
    #[error("the requested resource was not found")]
    NotFound,
    #[error("identity provider rejected the stored credentials: {reason}")]
    AuthRejected { reason: String },
    #[error("task join failed")]
    Join(#[from] tokio::task::JoinError),
    #[error("no session selected")]
    NoActiveSession,
    #[error("authenticated account epoch is exhausted")]
    AccountEpochExhausted,
    #[error("channel closed for session `{session}`")]
    ChannelClosed { session: String },
    #[error("channel full for session `{session}`")]
    ChannelFull { session: String },
    #[error("delivery outcome is unknown: {reason}")]
    DeliveryUnknown { reason: String },
    #[error("invalid terminal size {rows}x{cols}")]
    InvalidTerminalSize { rows: u16, cols: u16 },
    #[error("invalid URL `{value}`: {reason}")]
    InvalidUrl { value: String, reason: String },
    #[error("{key} is not configured")]
    MissingConfig { key: &'static str },
    #[error("backend returned invalid data for `{field}`: {reason}")]
    InvalidBackendData { field: String, reason: String },
    #[error("unsupported operation: {reason}")]
    Unsupported { reason: String },
    #[error("local device identity recovery is required: {reason}")]
    IdentityRecoveryRequired { reason: String },
    #[error(
        "Can't verify that user's identity on this device anymore ({detail}). \
        They may have reset their kodosi account from another device. \
        Sign out and sign back in to clear local trust, then ask them to share again."
    )]
    PeerIdentityChanged { user_id: String, detail: String },
}

impl AppError {
    #[cfg(any(test, feature = "cli"))]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::ConfigSource(_) | Self::ConfigDeserialize(_) => "configuration_error",
            Self::Io(_) => "io_error",
            Self::Http(_) | Self::HttpProblem { .. } => "http_error",
            Self::WebSocket(_) => "websocket_error",
            Self::Json(_) => "json_error",
            Self::Keychain { .. } => "credential_store_error",
            Self::Unauthorized | Self::AuthRejected { .. } => "unauthorized",
            Self::NotFound => "not_found",
            Self::Join(_) => "task_join_error",
            Self::NoActiveSession => "no_active_session",
            Self::AccountEpochExhausted => "account_epoch_exhausted",
            Self::ChannelClosed { .. } => "channel_closed",
            Self::ChannelFull { .. } => "channel_full",
            Self::DeliveryUnknown { .. } => "delivery_unknown",
            Self::InvalidTerminalSize { .. } => "invalid_terminal_size",
            Self::InvalidUrl { .. } => "invalid_url",
            Self::MissingConfig { .. } => "missing_configuration",
            Self::InvalidBackendData { .. } => "invalid_data",
            Self::Unsupported { .. } => "unsupported",
            Self::IdentityRecoveryRequired { .. } => "identity_recovery_required",
            Self::PeerIdentityChanged { .. } => "peer_identity_changed",
        }
    }
}

impl From<TerminalSizeError> for AppError {
    fn from(error: TerminalSizeError) -> Self {
        Self::InvalidTerminalSize {
            rows: error.rows,
            cols: error.cols,
        }
    }
}

impl From<SessionIdParseError> for AppError {
    fn from(error: SessionIdParseError) -> Self {
        Self::InvalidBackendData {
            field: error.field.to_owned(),
            reason: error.source.to_string(),
        }
    }
}

impl From<BackendClientError> for AppError {
    fn from(error: BackendClientError) -> Self {
        match error {
            BackendClientError::Io(error) => Self::Io(error),
            BackendClientError::Http(error) => Self::Http(error),
            BackendClientError::HttpProblem {
                status,
                code,
                detail,
            } => Self::HttpProblem {
                status,
                code,
                detail,
            },
            BackendClientError::WebSocket(error) => Self::WebSocket(error),
            BackendClientError::Json(error) => Self::Json(error),
            BackendClientError::Unauthorized => Self::Unauthorized,
            BackendClientError::Timeout { operation } => Self::Unsupported {
                reason: format!("backend operation timed out: {operation}"),
            },
            BackendClientError::NotFound => Self::NotFound,
            BackendClientError::MissingConfig { key } => Self::MissingConfig { key },
            BackendClientError::InvalidBackendData { field, reason } => {
                Self::InvalidBackendData { field, reason }
            }
            BackendClientError::Unsupported { reason }
            | BackendClientError::Protocol { reason }
            | BackendClientError::Crypto { reason }
            | BackendClientError::Capacity { reason, .. } => Self::Unsupported { reason },
            BackendClientError::RelayReservationExhausted { kind } => Self::Unsupported {
                reason: format!("host relay frame reservation exhausted: {kind:?}"),
            },
            BackendClientError::InvalidTerminalSize { rows, cols } => {
                Self::InvalidTerminalSize { rows, cols }
            }
            BackendClientError::InvalidUrl { value, reason } => Self::InvalidUrl { value, reason },
            BackendClientError::Auth(error) => app_error_from_boxed_source(error.into_source()),
            BackendClientError::HostRelayPort(error) => {
                app_error_from_boxed_source(error.into_source())
            }
            BackendClientError::SessionKeyTrust(error) => {
                app_error_from_boxed_source(error.into_source())
            }
        }
    }
}

fn app_error_from_boxed_source(
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
) -> AppError {
    match source.downcast::<AppError>() {
        Ok(error) => *error,
        Err(source) => AppError::Unsupported {
            reason: source.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn account_epoch_and_channel_error_codes_remain_distinct() {
        assert_eq!(
            AppError::AccountEpochExhausted.code(),
            "account_epoch_exhausted"
        );
        assert_eq!(
            AppError::ChannelClosed {
                session: "session-1".to_owned(),
            }
            .code(),
            "channel_closed"
        );
        assert_eq!(
            AppError::ChannelFull {
                session: "session-1".to_owned(),
            }
            .code(),
            "channel_full"
        );
    }
}
