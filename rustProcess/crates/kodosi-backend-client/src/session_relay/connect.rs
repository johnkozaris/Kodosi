use zeroize::Zeroizing;

use crate::{
    BackendClientError, Result,
    auth::{BackendAccessTokenState, BackendAuthProvider},
    crypto,
    http_client::BackendHttpClient,
    session_key_service::{
        SessionKeyFetchOutcome, SessionKeyTrustAnchor, SessionKeyTrustContext, fetch_session_key,
    },
};
use kodosi_domain::lifecycle::RemoteSessionAccessState;

use super::{
    SessionRelayClientSpec,
    cursor::ReplayCursor,
    events::SessionRelayEventSink,
    ws::{SessionRelayWebSocketStream, SessionRelayWsClient},
};

pub(super) struct ConnectedRemoteSession {
    pub stream: SessionRelayWebSocketStream,
    pub fetched_key: Option<crypto::SessionKey>,
    pub trusted_signer: Option<crate::session_key_service::SessionKeyTrustedSigner>,
    pub retry_missing_session_key: bool,
    pub owner_pin_established: bool,
    pub refreshed: bool,
    pub access_token: Zeroizing<String>,
}

pub(super) enum ConnectAttemptOutcome {
    Connected(Box<ConnectedRemoteSession>),
    Waiting,
    RetryWithRejectedToken(Zeroizing<String>),
}

#[derive(Debug, PartialEq, Eq)]
enum MissingSessionKeyPolicy {
    RetryReceiveLoop,
    StopBeforeReceiveLoop,
}

fn missing_session_key_policy(retryable: bool) -> MissingSessionKeyPolicy {
    if retryable {
        MissingSessionKeyPolicy::RetryReceiveLoop
    } else {
        MissingSessionKeyPolicy::StopBeforeReceiveLoop
    }
}

async fn connect(
    spec: &SessionRelayClientSpec,
    backend: &mut BackendHttpClient,
    session_relay_ws: &SessionRelayWsClient,
    access_token: &str,
    cursor: ReplayCursor,
) -> Result<SessionRelayWebSocketStream> {
    backend.set_access_token(Some(Zeroizing::new(access_token.to_owned())));
    session_relay_ws
        .connect_participant(
            &spec.backend_session_id,
            access_token,
            cursor,
            spec.viewer_device_id
                .as_deref()
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: "participant relay requires a registered device id".to_owned(),
                })?,
            spec.viewer_user_id
                .as_deref()
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: "participant relay requires an authenticated user id".to_owned(),
                })?,
            spec.viewer_signing_pkcs8
                .as_deref()
                .ok_or_else(|| BackendClientError::Protocol {
                    reason: "participant relay requires a device signing key".to_owned(),
                })?,
            &spec.backend_incarnation_id,
        )
        .await
}

#[expect(clippy::too_many_arguments, reason = "flat relay-setup contract")]
async fn connect_remote_session(
    spec: &SessionRelayClientSpec,
    backend: &mut BackendHttpClient,
    trust_anchor: &dyn SessionKeyTrustAnchor,
    session_relay_ws: &SessionRelayWsClient,
    events: &SessionRelayEventSink,
    access_token: &str,
    cursor: ReplayCursor,
    refreshed: bool,
    first_connect: bool,
) -> Result<Option<ConnectedRemoteSession>> {
    let stream = connect(spec, backend, session_relay_ws, access_token, cursor).await?;

    let mut fetched_key = None;
    let mut trusted_signer = None;
    let mut retry_missing_session_key = false;

    {
        let Some(owner_user_id) = spec.owner_user_id.as_deref() else {
            events
                .access_state(
                    RemoteSessionAccessState::Failed,
                    Some("owner user id not available for session-key trust anchor".to_owned()),
                    None,
                )
                .await;
            return Ok(None);
        };
        let trust_context = if first_connect {
            SessionKeyTrustContext::ExplicitShare
        } else {
            SessionKeyTrustContext::BackgroundFetch
        };
        match fetch_session_key(
            backend,
            trust_anchor,
            &spec.backend_session_id,
            &spec.backend_incarnation_id,
            owner_user_id,
            spec.viewer_kem_secret_bytes.as_deref(),
            spec.viewer_device_id.as_deref(),
            spec.viewer_user_id.as_deref(),
            spec.viewer_signing_pkcs8.as_deref(),
            trust_context,
        )
        .await
        {
            SessionKeyFetchOutcome::Ready {
                key,
                trusted_signer: fetched_trusted_signer,
            } => {
                events
                    .remote_control_trust(spec.backend_incarnation_id, &fetched_trusted_signer)
                    .await;
                trusted_signer = Some(fetched_trusted_signer);
                events
                    .access_state(RemoteSessionAccessState::Ready, None, None)
                    .await;
                fetched_key = Some(key);
            }
            SessionKeyFetchOutcome::Unavailable {
                state,
                reason,
                issue,
                retryable,
            } => {
                events.access_state(state, Some(reason), issue).await;
                match missing_session_key_policy(retryable) {
                    MissingSessionKeyPolicy::RetryReceiveLoop => {
                        retry_missing_session_key = true;
                    }
                    MissingSessionKeyPolicy::StopBeforeReceiveLoop => return Ok(None),
                }
            }
        }
    }

    Ok(Some(ConnectedRemoteSession {
        stream,
        fetched_key,
        trusted_signer,
        retry_missing_session_key,
        owner_pin_established: fetched_key.is_some(),
        refreshed,
        access_token: Zeroizing::new(access_token.to_owned()),
    }))
}

#[expect(clippy::too_many_arguments, reason = "mirrors connect_remote_session")]
pub(super) async fn attempt_connect_cycle(
    spec: &SessionRelayClientSpec,
    backend: &mut BackendHttpClient,
    trust_anchor: &dyn SessionKeyTrustAnchor,
    session_relay_ws: &SessionRelayWsClient,
    auth_provider: &BackendAuthProvider,
    events: &SessionRelayEventSink,
    cursor: ReplayCursor,
    awaiting_reauthentication: &mut bool,
    first_connect: bool,
    force_refresh_token: Option<&str>,
) -> ConnectAttemptOutcome {
    let token_state = auth_provider
        .access_token(force_refresh_token.map(str::to_owned))
        .await;
    match token_state {
        Ok(BackendAccessTokenState::Ready {
            access_token,
            refreshed,
            ..
        }) => {
            *awaiting_reauthentication = false;
            match connect_remote_session(
                spec,
                backend,
                trust_anchor,
                session_relay_ws,
                events,
                &access_token,
                cursor,
                refreshed,
                first_connect,
            )
            .await
            {
                Ok(Some(connected)) => ConnectAttemptOutcome::Connected(Box::new(connected)),
                Ok(None) => ConnectAttemptOutcome::Waiting,
                Err(error) => {
                    let unauthorized = error.is_websocket_unauthorized();
                    events
                        .log_error(format!("session relay connect failed: {error}"))
                        .await;
                    if unauthorized && force_refresh_token.is_none() {
                        ConnectAttemptOutcome::RetryWithRejectedToken(access_token)
                    } else {
                        ConnectAttemptOutcome::Waiting
                    }
                }
            }
        }
        Ok(BackendAccessTokenState::RequiresLogin { reason }) => {
            if !*awaiting_reauthentication {
                events.backend_access_invalid(reason.to_string()).await;
                events
                    .info(format!("session relay is waiting for sign-in: {reason}"))
                    .await;
                *awaiting_reauthentication = true;
            }
            ConnectAttemptOutcome::Waiting
        }
        Ok(BackendAccessTokenState::TemporarilyUnavailable { reason }) => {
            events
                .info(format!(
                    "session relay is waiting for auth refresh: {reason}"
                ))
                .await;
            ConnectAttemptOutcome::Waiting
        }
        Err(error) => {
            events
                .log_error(format!("session relay auth check failed: {error}"))
                .await;
            ConnectAttemptOutcome::Waiting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MissingSessionKeyPolicy, missing_session_key_policy};

    #[test]
    fn non_retryable_missing_session_key_stops_before_receive_loop() {
        assert_eq!(
            missing_session_key_policy(false),
            MissingSessionKeyPolicy::StopBeforeReceiveLoop
        );
    }

    #[test]
    fn retryable_missing_session_key_keeps_receive_loop_for_refetch() {
        assert_eq!(
            missing_session_key_policy(true),
            MissingSessionKeyPolicy::RetryReceiveLoop
        );
    }
}
