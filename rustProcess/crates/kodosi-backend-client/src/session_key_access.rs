use reqwest::StatusCode;

use crate::{
    BackendClientError, dto::session_keys::SessionKeyFetchStateDto,
    session_key_service::SessionKeyTrustFailure,
};
use kodosi_domain::lifecycle::{RemoteSessionAccessIssue, RemoteSessionAccessState};

pub(crate) const WAITING_FOR_SESSION_KEY_REASON: &str =
    "Waiting for an encrypted session key from the owner.";
const ENCRYPTED_SESSION_SIGN_IN_REQUIRED_REASON: &str =
    "Sign in is required to view encrypted sessions.";
const ENCRYPTED_SESSION_ACCESS_DENIED_REASON: &str =
    "You no longer have access to this encrypted session.";
const ENCRYPTED_SESSION_UNKNOWN_DEVICE_REASON: &str =
    "This device is not registered for encrypted session access.";
const ENCRYPTED_SESSION_NOT_LIVE_REASON: &str = "This encrypted session is no longer live.";
const ENCRYPTED_SESSION_SETUP_FAILED_REASON: &str = "Encrypted session setup failed.";

pub(crate) struct SessionKeyFetchFailure {
    pub state: RemoteSessionAccessState,
    pub reason: String,
    pub issue: Option<RemoteSessionAccessIssue>,
}

pub(crate) fn session_key_fetch_failure(error: &BackendClientError) -> SessionKeyFetchFailure {
    SessionKeyFetchFailure {
        state: session_key_fetch_error_state(error),
        reason: format_session_key_fetch_error(error),
        issue: session_key_fetch_error_issue(error),
    }
}

pub(crate) fn format_session_key_fetch_error(error: &BackendClientError) -> String {
    match error {
        BackendClientError::Unauthorized => ENCRYPTED_SESSION_SIGN_IN_REQUIRED_REASON.to_owned(),

        BackendClientError::HttpProblem { detail, .. } => humanize_reason(detail),
        BackendClientError::Http(http_error) => http_error.status().map_or_else(
            || ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned(),
            format_session_key_http_status,
        ),
        BackendClientError::Unsupported { reason }
        | BackendClientError::Protocol { reason }
        | BackendClientError::Crypto { reason }
        | BackendClientError::Capacity { reason, .. }
        | BackendClientError::InvalidBackendData { reason, .. } => humanize_reason(reason),
        BackendClientError::SessionKeyTrust(error)
            if error.failure() == SessionKeyTrustFailure::Unauthorized =>
        {
            ENCRYPTED_SESSION_SIGN_IN_REQUIRED_REASON.to_owned()
        }
        BackendClientError::SessionKeyTrust(error)
            if error.failure() == SessionKeyTrustFailure::PeerIdentityChanged =>
        {
            "This friend's account identity changed. \
             Trust again to re-pin, or ask them to share with you once more."
                .to_owned()
        }
        BackendClientError::SessionKeyTrust(error) => humanize_reason(&error.source_message()),
        other => {
            let message = other.to_string();
            if message.is_empty() {
                ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned()
            } else {
                message
            }
        }
    }
}

pub(crate) fn format_session_key_fetch_state_reason(
    state: &SessionKeyFetchStateDto,
) -> &'static str {
    match state {
        SessionKeyFetchStateDto::Ready => ENCRYPTED_SESSION_SETUP_FAILED_REASON,
        SessionKeyFetchStateDto::PendingDistribution => WAITING_FOR_SESSION_KEY_REASON,
        SessionKeyFetchStateDto::UnknownDevice => ENCRYPTED_SESSION_UNKNOWN_DEVICE_REASON,
        SessionKeyFetchStateDto::SessionNotLive => ENCRYPTED_SESSION_NOT_LIVE_REASON,
    }
}

pub(crate) fn session_key_fetch_access_state(
    state: &SessionKeyFetchStateDto,
) -> RemoteSessionAccessState {
    match state {
        SessionKeyFetchStateDto::PendingDistribution => RemoteSessionAccessState::AwaitingKey,

        SessionKeyFetchStateDto::Ready
        | SessionKeyFetchStateDto::UnknownDevice
        | SessionKeyFetchStateDto::SessionNotLive => RemoteSessionAccessState::Failed,
    }
}

pub(crate) fn session_key_fetch_state_retryable(state: &SessionKeyFetchStateDto) -> bool {
    matches!(state, SessionKeyFetchStateDto::PendingDistribution)
}

pub(crate) fn session_key_fetch_error_state(
    error: &BackendClientError,
) -> RemoteSessionAccessState {
    match error {
        BackendClientError::SessionKeyTrust(error)
            if error.failure() == SessionKeyTrustFailure::PeerIdentityChanged =>
        {
            RemoteSessionAccessState::AccessDenied
        }
        BackendClientError::Unsupported { .. }
        | BackendClientError::Protocol { .. }
        | BackendClientError::Crypto { .. }
        | BackendClientError::Capacity { .. }
        | BackendClientError::RelayReservationExhausted { .. }
        | BackendClientError::InvalidBackendData { .. }
        | BackendClientError::HttpProblem { .. }
        | BackendClientError::Unauthorized
        | BackendClientError::NotFound
        | BackendClientError::Http(_)
        | BackendClientError::Json(_)
        | BackendClientError::Io(_)
        | BackendClientError::WebSocket(_)
        | BackendClientError::Timeout { .. }
        | BackendClientError::InvalidTerminalSize { .. }
        | BackendClientError::InvalidUrl { .. }
        | BackendClientError::MissingConfig { .. }
        | BackendClientError::SessionKeyTrust(_)
        | BackendClientError::HostRelayPort(_)
        | BackendClientError::Auth(_) => RemoteSessionAccessState::Failed,
    }
}

fn session_key_fetch_error_issue(error: &BackendClientError) -> Option<RemoteSessionAccessIssue> {
    match error {
        BackendClientError::SessionKeyTrust(error)
            if error.failure() == SessionKeyTrustFailure::PeerIdentityChanged =>
        {
            Some(RemoteSessionAccessIssue::PeerIdentityChanged)
        }
        _ => None,
    }
}

fn format_session_key_http_status(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => ENCRYPTED_SESSION_SIGN_IN_REQUIRED_REASON.to_owned(),
        StatusCode::FORBIDDEN => ENCRYPTED_SESSION_ACCESS_DENIED_REASON.to_owned(),
        other => format!("Encrypted session setup failed ({other})."),
    }
}

fn humanize_reason(reason: &str) -> String {
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        return ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned();
    }

    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned();
    };
    let mut result = first.to_uppercase().collect::<String>();
    result.push_str(chars.as_str());
    result
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::{
        ENCRYPTED_SESSION_NOT_LIVE_REASON, ENCRYPTED_SESSION_UNKNOWN_DEVICE_REASON,
        WAITING_FOR_SESSION_KEY_REASON, format_session_key_fetch_error,
        format_session_key_fetch_state_reason, format_session_key_http_status,
        session_key_fetch_error_state, session_key_fetch_failure,
    };
    use crate::{
        BackendClientError,
        dto::session_keys::SessionKeyFetchStateDto,
        session_key_service::{SessionKeyTrustError, SessionKeyTrustFailure},
    };
    use kodosi_domain::lifecycle::{RemoteSessionAccessIssue, RemoteSessionAccessState};

    #[test]
    fn session_key_fetch_state_messages_match_browser_copy() {
        assert_eq!(
            format_session_key_fetch_state_reason(&SessionKeyFetchStateDto::PendingDistribution),
            WAITING_FOR_SESSION_KEY_REASON
        );
        assert_eq!(
            format_session_key_fetch_state_reason(&SessionKeyFetchStateDto::UnknownDevice),
            ENCRYPTED_SESSION_UNKNOWN_DEVICE_REASON
        );
        assert_eq!(
            format_session_key_fetch_state_reason(&SessionKeyFetchStateDto::SessionNotLive),
            ENCRYPTED_SESSION_NOT_LIVE_REASON
        );
    }

    #[test]
    fn session_key_http_status_messages_match_browser_copy() {
        assert_eq!(
            format_session_key_http_status(StatusCode::UNAUTHORIZED),
            "Sign in is required to view encrypted sessions."
        );
        assert_eq!(
            format_session_key_http_status(StatusCode::FORBIDDEN),
            "You no longer have access to this encrypted session."
        );
    }

    #[test]
    fn unsupported_session_key_errors_are_humanized() {
        assert_eq!(
            format_session_key_fetch_error(&BackendClientError::Protocol {
                reason: "session key blob is missing signature".to_owned(),
            }),
            "Session key blob is missing signature"
        );
    }

    #[test]
    fn peer_identity_changed_errors_keep_clean_message_and_typed_issue() {
        let failure = session_key_fetch_failure(&BackendClientError::SessionKeyTrust(
            SessionKeyTrustError::from_error(
                SessionKeyTrustFailure::PeerIdentityChanged,
                std::io::Error::other("stale pin"),
            ),
        ));

        assert_eq!(failure.state, RemoteSessionAccessState::AccessDenied);
        assert_eq!(
            failure.issue,
            Some(RemoteSessionAccessIssue::PeerIdentityChanged)
        );
        assert!(!failure.reason.contains("code:"));
    }

    #[test]
    fn peer_identity_changed_errors_preserve_access_denied_state() {
        assert_eq!(
            session_key_fetch_error_state(&BackendClientError::SessionKeyTrust(
                SessionKeyTrustError::from_error(
                    SessionKeyTrustFailure::PeerIdentityChanged,
                    std::io::Error::other("stale pin"),
                ),
            )),
            RemoteSessionAccessState::AccessDenied
        );
    }

    #[test]
    fn wrapped_trust_unauthorized_errors_keep_sign_in_copy() {
        assert_eq!(
            format_session_key_fetch_error(&BackendClientError::SessionKeyTrust(
                SessionKeyTrustError::from_error(
                    SessionKeyTrustFailure::Unauthorized,
                    std::io::Error::other("backend rejected the current credentials"),
                ),
            )),
            "Sign in is required to view encrypted sessions."
        );
    }
}
