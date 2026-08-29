use std::fmt;

use reqwest::{StatusCode, Url};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{
        self,
        http::{Response, header::WWW_AUTHENTICATE},
    },
};

use crate::{
    Result,
    endpoint::{join_endpoint, parse_url, set_tcp_nodelay, user_events_ws_config},
};

pub type UserEventsWebSocketStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserEventsConnectFailureKind {
    AuthRejected,
    Http(StatusCode),
    Transport,
    Configuration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEventsConnectFailure {
    kind: UserEventsConnectFailureKind,
    detail: String,
}

impl UserEventsConnectFailure {
    pub fn is_auth_rejected(&self) -> bool {
        matches!(self.kind, UserEventsConnectFailureKind::AuthRejected)
    }

    pub fn reason(&self) -> &str {
        &self.detail
    }

    pub fn is_configuration(&self) -> bool {
        matches!(self.kind, UserEventsConnectFailureKind::Configuration)
    }

    fn configuration(endpoint: &str, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            kind: UserEventsConnectFailureKind::Configuration,
            detail: format!("user events websocket at {endpoint} could not start: {reason}"),
        }
    }

    fn transport(endpoint: &Url, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            kind: UserEventsConnectFailureKind::Transport,
            detail: format!("user events websocket at {endpoint} failed: {reason}"),
        }
    }

    fn http(endpoint: &Url, response: &Response<Option<Vec<u8>>>) -> Self {
        let status = response.status();
        let auth_rejected = matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN);
        let qualifier = if auth_rejected {
            "rejected the current credentials"
        } else {
            "returned an unexpected handshake response"
        };
        let mut detail = format!(
            "user events websocket at {endpoint} {qualifier}: HTTP {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown Status"),
        );

        let mut extras = Vec::new();
        if let Some(header) = response
            .headers()
            .get(WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok())
        {
            let trimmed = header.trim();
            if !trimmed.is_empty() {
                extras.push(format!("www-authenticate={trimmed}"));
            }
        }
        if let Some(body) = summarize_response_body(response.body().as_deref()) {
            extras.push(format!("body={body}"));
        }
        if let Some(hint) = handshake_status_hint(status) {
            extras.push(format!("hint={hint}"));
        }
        if !extras.is_empty() {
            detail.push_str(" (");
            detail.push_str(&extras.join("; "));
            detail.push(')');
        }

        Self {
            kind: if auth_rejected {
                UserEventsConnectFailureKind::AuthRejected
            } else {
                UserEventsConnectFailureKind::Http(status)
            },
            detail,
        }
    }

    fn from_tungstenite(endpoint: &Url, error: tungstenite::Error) -> Self {
        match error {
            tungstenite::Error::Http(response) => Self::http(endpoint, &response),
            tungstenite::Error::Io(source) => {
                Self::transport(endpoint, format!("{source} [kind={}]", source.kind()))
            }
            other => Self::transport(endpoint, other.to_string()),
        }
    }
}

impl fmt::Display for UserEventsConnectFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for UserEventsConnectFailure {}

#[derive(Debug, Clone)]
pub struct UserEventsWsClient {
    base_url: Option<Url>,
}

impl UserEventsWsClient {
    pub fn new(base_url: Option<&str>) -> Result<Self> {
        Ok(Self {
            base_url: base_url.map(parse_url).transpose()?,
        })
    }

    pub async fn connect(
        &self,
        access_token: &str,
        device_id: &str,
        user_id: &str,
        signing_pkcs8: &[u8],
    ) -> std::result::Result<UserEventsWebSocketStream, UserEventsConnectFailure> {
        let url = join_endpoint(self.base_url.as_ref(), "me/events", "backend.user_events")
            .map_err(|error| {
                let endpoint = self.base_url.as_ref().map_or_else(
                    || "backend.user_events".to_owned(),
                    |base_url| format!("{base_url}/me/events"),
                );
                UserEventsConnectFailure::configuration(&endpoint, error.to_string())
            })?;
        let endpoint = url.to_string();
        let request =
            crate::endpoint::build_ws_bearer_request(&url, access_token).map_err(|error| {
                UserEventsConnectFailure::configuration(&endpoint, error.to_string())
            })?;
        let (mut stream, _) =
            connect_async_with_config(request, Some(user_events_ws_config()), false)
                .await
                .map_err(|error| UserEventsConnectFailure::from_tungstenite(&url, error))?;
        set_tcp_nodelay(&stream).map_err(|error| {
            UserEventsConnectFailure::transport(
                &url,
                format!("connected, but could not enable TCP_NODELAY: {error}"),
            )
        })?;
        crate::host_ws::prove_device_connection(
            &mut stream,
            user_id,
            device_id,
            signing_pkcs8,
            "user-events",
            None,
            None,
        )
        .await
        .map_err(|error| UserEventsConnectFailure::transport(&url, error.to_string()))?;
        Ok(stream)
    }
}

fn summarize_response_body(body: Option<&[u8]>) -> Option<String> {
    const MAX_BODY_CHARS: usize = 160;

    let body = body?;
    let collapsed = String::from_utf8_lossy(body)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.is_empty() {
        return None;
    }

    let mut preview = collapsed.chars().take(MAX_BODY_CHARS).collect::<String>();
    if collapsed.chars().count() > MAX_BODY_CHARS {
        preview.push('…');
    }
    Some(preview)
}

fn handshake_status_hint(status: StatusCode) -> Option<&'static str> {
    match status {
        StatusCode::UNAUTHORIZED => Some("the access token is missing, expired, or rejected"),
        StatusCode::FORBIDDEN => {
            Some("the device is not active or is absent from the signed device list")
        }
        StatusCode::SERVICE_UNAVAILABLE => Some("the backend is draining or still starting up"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Url;
    use tokio_tungstenite::tungstenite::http::{Response, StatusCode};

    use super::{UserEventsConnectFailure, UserEventsConnectFailureKind};

    #[test]
    fn unauthorized_handshake_is_classified_as_auth_rejection() {
        let endpoint =
            Url::parse("ws://localhost:5050/me/events").unwrap_or_else(|error| panic!("{error}"));
        let response = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("www-authenticate", "Bearer error=\"invalid_token\"")
            .body(None)
            .unwrap_or_else(|error| panic!("{error}"));

        let failure = UserEventsConnectFailure::http(&endpoint, &response);

        assert!(failure.is_auth_rejected());
        assert_eq!(failure.kind, UserEventsConnectFailureKind::AuthRejected);
        assert!(failure.reason().contains("HTTP 401 Unauthorized"));
        assert!(
            failure
                .reason()
                .contains("www-authenticate=Bearer error=\"invalid_token\"")
        );
        assert!(
            failure
                .reason()
                .contains("hint=the access token is missing, expired, or rejected")
        );
    }

    #[test]
    fn forbidden_handshake_is_classified_as_device_access_rejection() {
        let endpoint =
            Url::parse("ws://localhost:5050/me/events").unwrap_or_else(|error| panic!("{error}"));
        let response = Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(None)
            .unwrap_or_else(|error| panic!("{error}"));

        let failure = UserEventsConnectFailure::http(&endpoint, &response);

        assert!(failure.is_auth_rejected());
        assert_eq!(failure.kind, UserEventsConnectFailureKind::AuthRejected);
        assert!(
            failure
                .reason()
                .contains("hint=the device is not active or is absent from the signed device list")
        );
    }

    #[test]
    fn non_auth_handshake_preserves_response_body_context() {
        let endpoint =
            Url::parse("ws://localhost:5050/me/events").unwrap_or_else(|error| panic!("{error}"));
        let response = Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Some(b"relay startup is still warming up".to_vec()))
            .unwrap_or_else(|error| panic!("{error}"));

        let failure = UserEventsConnectFailure::http(&endpoint, &response);

        assert_eq!(
            failure.kind,
            UserEventsConnectFailureKind::Http(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert!(failure.reason().contains("HTTP 503 Service Unavailable"));
        assert!(
            failure
                .reason()
                .contains("body=relay startup is still warming up")
        );
    }
}
