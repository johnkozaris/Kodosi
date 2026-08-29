use std::io;

use thiserror::Error;

use kodosi_domain::{ids::SessionIdParseError, terminal::TerminalSizeError};

pub type Result<T> = std::result::Result<T, BackendClientError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayReservationKind {
    Nonce,
    Revision,
}

#[derive(Debug, Error)]
pub enum BackendClientError {
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
    #[error("backend operation `{operation}` timed out")]
    Timeout { operation: &'static str },
    #[error("JSON processing failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("backend rejected the current credentials")]
    Unauthorized,
    #[error("the requested resource was not found")]
    NotFound,
    #[error("{key} is not configured")]
    MissingConfig { key: &'static str },
    #[error("backend returned invalid data for `{field}`: {reason}")]
    InvalidBackendData { field: String, reason: String },
    #[error("backend protocol violation: {reason}")]
    Protocol { reason: String },
    #[error("cryptographic operation failed: {reason}")]
    Crypto { reason: String },
    #[error("unsupported operation: {reason}")]
    Unsupported { reason: String },
    #[error("{resource} exhausted: {reason}")]
    Capacity {
        resource: &'static str,
        reason: String,
    },
    #[error("host relay frame reservation exhausted: {kind:?}")]
    RelayReservationExhausted { kind: RelayReservationKind },
    #[error("invalid terminal size {rows}x{cols}")]
    InvalidTerminalSize { rows: u16, cols: u16 },
    #[error("invalid URL `{value}`: {reason}")]
    InvalidUrl { value: String, reason: String },
    #[error(transparent)]
    Auth(#[from] super::auth::BackendAuthError),
    #[error(transparent)]
    HostRelayPort(#[from] super::relay::HostRelayPortError),
    #[error(transparent)]
    SessionKeyTrust(#[from] super::session_key_service::SessionKeyTrustError),
}

impl From<TerminalSizeError> for BackendClientError {
    fn from(error: TerminalSizeError) -> Self {
        Self::InvalidTerminalSize {
            rows: error.rows,
            cols: error.cols,
        }
    }
}

impl From<SessionIdParseError> for BackendClientError {
    fn from(error: SessionIdParseError) -> Self {
        Self::InvalidBackendData {
            field: error.field.to_owned(),
            reason: error.source.to_string(),
        }
    }
}

impl BackendClientError {
    #[must_use]
    pub fn is_indeterminate_write(&self) -> bool {
        matches!(
            self,
            Self::Io(_) | Self::Http(_) | Self::Timeout { .. } | Self::Json(_)
        )
    }

    #[must_use]
    pub(crate) fn is_relay_reservation_exhausted(&self) -> bool {
        matches!(self, Self::RelayReservationExhausted { .. })
    }

    #[must_use]
    pub(crate) fn is_websocket_unauthorized(&self) -> bool {
        matches!(
            self,
            Self::WebSocket(tokio_tungstenite::tungstenite::Error::Http(response))
                if response.status() == reqwest::StatusCode::UNAUTHORIZED
        )
    }
}

#[cfg(test)]
mod tests {
    use super::BackendClientError;

    #[test]
    fn websocket_http_unauthorized_requests_token_refresh() {
        let response = tokio_tungstenite::tungstenite::http::Response::builder()
            .status(reqwest::StatusCode::UNAUTHORIZED)
            .body(None)
            .expect("response");
        let error = BackendClientError::WebSocket(tokio_tungstenite::tungstenite::Error::Http(
            Box::new(response),
        ));

        assert!(error.is_websocket_unauthorized());
    }
}
