use std::{fmt, future::Future, sync::Arc, time::Duration};

use ::time::OffsetDateTime;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::{self, Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::{AppError, Result, config::AuthConfig};

#[derive(Debug, Clone, Deserialize)]
struct OidcDiscovery {
    token_endpoint: String,
    device_authorization_endpoint: Option<String>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct DeviceAuthorizationResponse {
    pub(crate) device_code: String,
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) verification_uri_complete: Option<String>,
    pub(crate) expires_in: i64,
    pub(crate) interval: Option<u64>,
    #[serde(skip)]
    pub(crate) token_endpoint: String,
}

impl fmt::Debug for DeviceAuthorizationResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceAuthorizationResponse")
            .field("device_code", &"<redacted>")
            .field("user_code", &"<redacted>")
            .field("verification_uri", &self.verification_uri)
            .field(
                "verification_uri_complete",
                &self
                    .verification_uri_complete
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

impl DeviceAuthorizationResponse {
    pub(crate) fn launch_uri(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .unwrap_or(&self.verification_uri)
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct TokenResponse {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: i64,
    #[expect(
        dead_code,
        reason = "parsed for protocol completeness; always 'Bearer' in Authentik responses"
    )]
    pub(crate) token_type: String,
}

impl fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TokenErrorResponse {
    error: String,
}

#[derive(Debug, Clone)]
pub(crate) enum DeviceFlowResult {
    Success(TokenResponse),
    Denied,
    Expired,
    Failed(DeviceFlowFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceFlowFailure {
    PollRequest(String),
    TokenResponseParse(String),
    ErrorResponseRead(String),
    TokenEndpoint(String),
}

impl fmt::Display for DeviceFlowFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PollRequest(reason) => write!(f, "token poll request failed: {reason}"),
            Self::TokenResponseParse(reason) => {
                write!(f, "failed to parse token response: {reason}")
            }
            Self::ErrorResponseRead(reason) => {
                write!(f, "failed to read error response: {reason}")
            }
            Self::TokenEndpoint(reason) => write!(f, "token endpoint error: {reason}"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceFlowClient {
    client: reqwest::Client,
    discovery: Arc<tokio::sync::OnceCell<OidcDiscovery>>,
    client_id: String,
    scope: String,
    audience: Option<String>,
    issuer: Option<Url>,
}

impl DeviceFlowClient {
    pub(crate) fn new(config: &AuthConfig) -> Result<Self> {
        let issuer = config
            .issuer
            .as_deref()
            .map(|s| {
                Url::parse(s).map_err(|e| AppError::InvalidUrl {
                    value: s.to_owned(),
                    reason: e.to_string(),
                })
            })
            .transpose()?;

        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(AppError::Http)?;

        Ok(Self {
            client,
            discovery: Arc::new(tokio::sync::OnceCell::new()),
            client_id: config.client_id.clone(),
            scope: config.scope.clone(),
            audience: config.audience.clone(),
            issuer,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(config: &AuthConfig, client: reqwest::Client) -> Result<Self> {
        let issuer = config
            .issuer
            .as_deref()
            .map(|value| {
                Url::parse(value).map_err(|error| AppError::InvalidUrl {
                    value: value.to_owned(),
                    reason: error.to_string(),
                })
            })
            .transpose()?;
        Ok(Self {
            client,
            discovery: Arc::new(tokio::sync::OnceCell::new()),
            client_id: config.client_id.clone(),
            scope: config.scope.clone(),
            audience: config.audience.clone(),
            issuer,
        })
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.issuer.is_some()
    }

    async fn discover(&self) -> Result<OidcDiscovery> {
        self.discovery
            .get_or_try_init(|| discover_oidc(&self.client, self.issuer.as_ref()))
            .await
            .cloned()
    }

    pub(crate) async fn prime_discovery(&self) {
        if !self.is_configured() {
            return;
        }
        if let Err(error) = self.discover().await {
            tracing::debug!(%error, "OIDC discovery prewarm failed; login will retry");
        }
    }

    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn start(&self) -> Result<DeviceAuthorizationResponse> {
        let discovery = self.discover().await?;
        let endpoint =
            discovery
                .device_authorization_endpoint
                .ok_or_else(|| AppError::Unsupported {
                    reason: "OIDC provider does not support device authorization flow".to_owned(),
                })?;

        let mut form = vec![
            ("client_id".to_owned(), self.client_id.clone()),
            ("scope".to_owned(), self.scope.clone()),
        ];
        if let Some(audience) = &self.audience {
            form.push(("audience".to_owned(), audience.clone()));
        }

        let mut response = self
            .client
            .post(&endpoint)
            .form(&form)
            .send()
            .await
            .map_err(AppError::Http)?
            .error_for_status()
            .map_err(AppError::Http)?
            .json::<DeviceAuthorizationResponse>()
            .await
            .map_err(AppError::Http)?;
        response.token_endpoint = discovery.token_endpoint;
        Ok(response)
    }

    pub(crate) fn spawn_poll(
        &self,
        device_code: String,
        token_endpoint: String,
        poll_interval: u64,
        expires_in_seconds: i64,
        cancellation: CancellationToken,
        result_tx: mpsc::Sender<DeviceFlowResult>,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.client.clone();
        let client_id = self.client_id.clone();
        let expires_at = poll_expiration_deadline(expires_in_seconds);

        tokio::spawn(async move {
            let base_poll_interval = poll_interval.max(1);
            let mut current_poll_interval = base_poll_interval;
            let mut interval = polling_interval(current_poll_interval);
            interval.tick().await;

            loop {
                tokio::select! {
                    () = cancellation.cancelled() => break,
                    () = time::sleep_until(expires_at) => {
                        drop(result_tx.send(DeviceFlowResult::Expired).await);
                        break;
                    }
                    _ = interval.tick() => {
                        let Some(outcome) = poll_once_before_expiry(
                            expires_at,
                            poll_token_once(&client, &token_endpoint, &client_id, &device_code),
                        ).await else {
                            drop(result_tx.send(DeviceFlowResult::Expired).await);
                            break;
                        };
                        match outcome {
                            PollOutcome::Pending => {
                                if current_poll_interval != base_poll_interval {
                                    current_poll_interval = base_poll_interval;
                                    interval = polling_interval(current_poll_interval);
                                    interval.tick().await;
                                }
                            }
                            PollOutcome::SlowDown => {
                                current_poll_interval = current_poll_interval.saturating_add(5);
                                interval = polling_interval(current_poll_interval);
                                interval.tick().await;
                            }
                            PollOutcome::Success(tokens) => {
                                drop(result_tx.send(DeviceFlowResult::Success(tokens)).await);
                                break;
                            }
                            PollOutcome::Denied => {
                                drop(result_tx.send(DeviceFlowResult::Denied).await);
                                break;
                            }
                            PollOutcome::Expired => {
                                drop(result_tx.send(DeviceFlowResult::Expired).await);
                                break;
                            }
                            PollOutcome::Transient(reason) => {
                                tracing::warn!(%reason, "transient device-token poll failure; retrying");
                                current_poll_interval = current_poll_interval
                                    .saturating_mul(2)
                                    .clamp(base_poll_interval, 30);
                                interval = polling_interval(current_poll_interval);
                                interval.tick().await;
                            }
                            PollOutcome::Error(msg) => {
                                drop(result_tx.send(DeviceFlowResult::Failed(msg)).await);
                                break;
                            }
                        }
                    }
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn refresh(&self, refresh_token: &str) -> Result<TokenResponse> {
        let discovery = self.discover().await?;
        let mut form = vec![
            ("client_id".to_owned(), self.client_id.clone()),
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("refresh_token".to_owned(), refresh_token.to_owned()),
        ];
        if let Some(audience) = &self.audience {
            form.push(("audience".to_owned(), audience.clone()));
        }

        let response = self
            .client
            .post(&discovery.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(AppError::Http)?;

        if response.status().is_success() {
            return response
                .json::<TokenResponse>()
                .await
                .map_err(AppError::Http);
        }

        let status = response.status();
        let body = response.text().await.map_err(AppError::Http)?;
        Err(token_endpoint_error(status, &body))
    }
}

fn polling_interval(seconds: u64) -> time::Interval {
    let mut interval = time::interval(Duration::from_secs(seconds.max(1)));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

fn poll_expiration_deadline(expires_in_seconds: i64) -> Instant {
    let clamped = u64::try_from(expires_in_seconds.max(0)).unwrap_or(0);
    Instant::now() + Duration::from_secs(clamped)
}

async fn poll_once_before_expiry<F>(expires_at: Instant, poll_future: F) -> Option<PollOutcome>
where
    F: Future<Output = PollOutcome>,
{
    tokio::select! {
        () = time::sleep_until(expires_at) => None,
        outcome = poll_future => Some(outcome),
    }
}

pub(crate) fn token_expires_at(expires_in_seconds: i64) -> OffsetDateTime {
    OffsetDateTime::now_utc() + ::time::Duration::seconds(expires_in_seconds)
}

#[derive(Debug)]
enum PollOutcome {
    Pending,
    SlowDown,
    Success(TokenResponse),
    Denied,
    Expired,
    Transient(DeviceFlowFailure),
    Error(DeviceFlowFailure),
}

#[tracing::instrument(skip_all, err)]
async fn discover_oidc(client: &reqwest::Client, issuer: Option<&Url>) -> Result<OidcDiscovery> {
    let issuer = issuer.ok_or_else(|| AppError::Unsupported {
        reason: "auth.issuer is not configured".to_owned(),
    })?;
    let url = issuer
        .join(".well-known/openid-configuration")
        .map_err(|e| AppError::InvalidUrl {
            value: issuer.to_string(),
            reason: e.to_string(),
        })?;

    client
        .get(url)
        .send()
        .await
        .map_err(AppError::Http)?
        .error_for_status()
        .map_err(AppError::Http)?
        .json::<OidcDiscovery>()
        .await
        .map_err(AppError::Http)
}

#[tracing::instrument(skip_all)]
async fn poll_token_once(
    client: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    device_code: &str,
) -> PollOutcome {
    let response = match client
        .post(token_endpoint)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return PollOutcome::Transient(DeviceFlowFailure::PollRequest(e.to_string()));
        }
    };

    if response.status().is_success() {
        return match response.json::<TokenResponse>().await {
            Ok(tokens) => PollOutcome::Success(tokens),
            Err(e) => PollOutcome::Error(DeviceFlowFailure::TokenResponseParse(e.to_string())),
        };
    }

    let status = response.status();
    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => {
            return PollOutcome::Transient(DeviceFlowFailure::ErrorResponseRead(e.to_string()));
        }
    };

    match parse_token_error_code(&body).as_deref() {
        Some("authorization_pending") => PollOutcome::Pending,
        Some("slow_down") => PollOutcome::SlowDown,
        Some("access_denied") => PollOutcome::Denied,
        Some("expired_token") => PollOutcome::Expired,
        _ if status.is_server_error() => PollOutcome::Transient(DeviceFlowFailure::TokenEndpoint(
            format!("temporary token endpoint status {status}"),
        )),
        _ if status == StatusCode::TOO_MANY_REQUESTS => PollOutcome::SlowDown,
        _ => PollOutcome::Error(DeviceFlowFailure::TokenEndpoint(body)),
    }
}

fn parse_token_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<TokenErrorResponse>(body)
        .ok()
        .map(|response| response.error)
}

fn token_endpoint_error(status: StatusCode, body: &str) -> AppError {
    if let Some(error_code) = parse_token_error_code(body) {
        if is_terminal_refresh_error(&error_code) {
            return AppError::AuthRejected { reason: error_code };
        }

        return AppError::Unsupported {
            reason: format!("token endpoint returned `{error_code}` ({status})"),
        };
    }

    AppError::Unsupported {
        reason: format!("token endpoint returned {status}: {body}"),
    }
}

fn is_terminal_refresh_error(error_code: &str) -> bool {
    matches!(
        error_code,
        "access_denied"
            | "expired_token"
            | "invalid_client"
            | "invalid_grant"
            | "invalid_request"
            | "unauthorized_client"
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::StatusCode;
    use tokio::time::{self, timeout};

    use super::{
        DeviceAuthorizationResponse, PollOutcome, TokenResponse, is_terminal_refresh_error,
        poll_expiration_deadline, poll_once_before_expiry, polling_interval, token_endpoint_error,
    };
    use crate::AppError;

    #[test]
    fn prefers_complete_verification_uri_for_launch() {
        let response = DeviceAuthorizationResponse {
            device_code: "device".to_owned(),
            user_code: "user".to_owned(),
            verification_uri: "https://example.com/device".to_owned(),
            verification_uri_complete: Some("https://example.com/device?user_code=user".to_owned()),
            expires_in: 600,
            interval: Some(5),
            token_endpoint: "https://example.com/token".to_owned(),
        };

        assert_eq!(
            response.launch_uri(),
            "https://example.com/device?user_code=user"
        );
    }

    #[test]
    fn debug_output_redacts_device_and_token_credentials() {
        let authorization = DeviceAuthorizationResponse {
            device_code: "device-secret".to_owned(),
            user_code: "user-secret".to_owned(),
            verification_uri: "https://example.com/device".to_owned(),
            verification_uri_complete: Some(
                "https://example.com/device?user_code=complete-secret".to_owned(),
            ),
            expires_in: 600,
            interval: Some(5),
            token_endpoint: "https://example.com/token".to_owned(),
        };
        let authorization_debug = format!("{authorization:?}");
        assert!(!authorization_debug.contains("device-secret"));
        assert!(!authorization_debug.contains("user-secret"));
        assert!(!authorization_debug.contains("complete-secret"));

        let tokens = TokenResponse {
            access_token: "access-secret".to_owned(),
            refresh_token: Some("refresh-secret".to_owned()),
            expires_in: 300,
            token_type: "Bearer".to_owned(),
        };
        let token_debug = format!("{tokens:?}");
        assert!(!token_debug.contains("access-secret"));
        assert!(!token_debug.contains("refresh-secret"));
        assert!(token_debug.contains("<redacted>"));
    }

    #[test]
    fn classifies_invalid_grant_refresh_error_as_auth_rejected() {
        let error = token_endpoint_error(StatusCode::BAD_REQUEST, r#"{"error":"invalid_grant"}"#);

        std::assert_matches!(
            error,
            AppError::AuthRejected { reason } if reason == "invalid_grant"
        );
    }

    #[test]
    fn classifies_server_failures_as_transient_protocol_errors() {
        let error = token_endpoint_error(StatusCode::BAD_GATEWAY, "upstream unavailable");

        std::assert_matches!(
            error,
            AppError::Unsupported { reason } if reason.contains("502")
        );
    }

    #[test]
    fn recognizes_terminal_refresh_error_codes() {
        assert!(is_terminal_refresh_error("invalid_grant"));
        assert!(!is_terminal_refresh_error("temporarily_unavailable"));
    }

    #[test]
    fn clamps_negative_poll_expiration_to_now() {
        let before = time::Instant::now();
        let deadline = poll_expiration_deadline(-30);

        assert!(deadline >= before);
        assert!(deadline <= before + Duration::from_millis(10));
    }

    #[tokio::test]
    async fn polling_interval_clamps_zero_to_one_second() {
        assert_eq!(polling_interval(0).period(), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn polling_interval_preserves_nonzero_seconds() {
        assert_eq!(polling_interval(7).period(), Duration::from_secs(7));
    }

    #[tokio::test]
    async fn poll_once_before_expiry_returns_none_when_deadline_wins() {
        let deadline = time::Instant::now() + Duration::from_millis(20);
        let wait = tokio::spawn(async move {
            poll_once_before_expiry(deadline, std::future::pending::<PollOutcome>()).await
        });

        let result = timeout(Duration::from_secs(1), wait)
            .await
            .expect("poll deadline should resolve promptly")
            .expect("join deadline task");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn poll_once_before_expiry_returns_poll_outcome_before_deadline() {
        let deadline = time::Instant::now() + Duration::from_secs(1);

        let outcome = poll_once_before_expiry(deadline, async { PollOutcome::Pending }).await;

        std::assert_matches!(outcome, Some(PollOutcome::Pending));
    }
}
