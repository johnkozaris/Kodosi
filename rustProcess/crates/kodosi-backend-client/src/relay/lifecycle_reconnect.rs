use crate::auth::BackendAccessTokenState;
use kodosi_domain::session::SessionState;

use super::{
    emit::{
        emit_backend_access_invalid, emit_host_frame_reservation_exhausted, emit_info,
        emit_log_error, emit_state,
    },
    lifecycle::{ConnectedOutcome, HostRelayLoop},
    wire::connect_and_prime,
};

const RECONNECT_AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

impl HostRelayLoop {
    pub(super) async fn retire_exhausted_reservation(
        &mut self,
        error: &crate::BackendClientError,
    ) -> ConnectedOutcome {
        self.fail_pending_semantic_receipt();
        self.fail_pending_action_result();
        self.connected = None;
        self.live_stream.disable();
        emit_state(
            self.event_sink.as_ref(),
            self.spec.id,
            SessionState::Reconnecting,
        )
        .await;
        emit_host_frame_reservation_exhausted(self.event_sink.as_ref(), self.spec.id).await;
        emit_log_error(
            self.event_sink.as_ref(),
            self.spec.id,
            format!("host relay reservation exhausted; restarting with fresh ranges: {error}"),
        )
        .await;
        ConnectedOutcome::Stop
    }

    pub(super) async fn mark_reconnecting(&mut self, message: String) -> ConnectedOutcome {
        self.fail_pending_semantic_receipt();
        self.fail_pending_action_result();
        self.connected = None;
        self.live_stream.disable();
        emit_state(
            self.event_sink.as_ref(),
            self.spec.id,
            SessionState::Reconnecting,
        )
        .await;
        emit_log_error(self.event_sink.as_ref(), self.spec.id, message).await;
        ConnectedOutcome::Reconnect
    }

    pub(super) async fn mark_terminal_closed(&mut self, reason: &str) -> ConnectedOutcome {
        self.fail_pending_semantic_receipt();
        self.fail_pending_action_result();
        self.live_stream.disable();
        emit_state(
            self.event_sink.as_ref(),
            self.spec.id,
            SessionState::Stopped,
        )
        .await;
        emit_info(
            self.event_sink.as_ref(),
            self.spec.id,
            format!("host relay closed by backend: {reason}"),
        )
        .await;
        ConnectedOutcome::Stop
    }

    pub(super) async fn reconnect_after_backoff(&mut self) -> bool {
        if !self
            .reconnect_backoff
            .wait_for_next_retry(&self.cancellation)
            .await
        {
            return false;
        }

        match self.next_token_state().await {
            Ok(BackendAccessTokenState::Ready {
                access_token,
                refreshed,
                ..
            }) => Box::pin(self.reconnect_with_token(access_token, refreshed)).await,
            Ok(BackendAccessTokenState::RequiresLogin { reason }) => {
                self.record_authentication_required(reason.to_string())
                    .await;
            }
            Ok(BackendAccessTokenState::TemporarilyUnavailable { reason }) => {
                self.record_auth_temporarily_unavailable(reason.to_string())
                    .await;
            }
            Err(error) => {
                self.reconnect_backoff.record_failure();
                self.emit_reconnecting_state().await;
                emit_log_error(
                    self.event_sink.as_ref(),
                    self.spec.id,
                    format!("host relay auth check failed: {error}"),
                )
                .await;
            }
        }
        true
    }

    async fn next_token_state(&mut self) -> crate::Result<BackendAccessTokenState> {
        let revoked_access_token = self
            .force_refresh_token
            .take()
            .map(|token| token.to_string());
        super::await_relay_operation(
            "host relay authentication",
            RECONNECT_AUTH_TIMEOUT,
            &self.cancellation,
            self.auth_provider.access_token(revoked_access_token),
        )
        .await?
        .map_err(Into::into)
    }

    async fn reconnect_with_token(
        &mut self,
        access_token: zeroize::Zeroizing<String>,
        refreshed: bool,
    ) {
        self.awaiting_reauthentication = false;
        match Box::pin(connect_and_prime(
            &self.spec,
            &self.host_ws,
            &access_token,
            &self.cancellation,
        ))
        .await
        {
            Ok(primed) => {
                let backend_restarted = primed.accepted.relay_epoch != self.relay_epoch;
                self.relay_epoch = primed.accepted.relay_epoch;
                self.connected = Some(primed.stream);
                self.last_access_token = Some(access_token);
                self.live_stream.disable();
                self.terminal.checkpoint_pending = true;
                self.terminal.presentation_pending = true;
                self.terminal.expected_raw_sequence = None;
                self.terminal.pending_raw.clear();
                self.pending_permissions.send_required = true;
                if backend_restarted {
                    self.event_sink
                        .emit(super::HostRelayEvent::BackendRelayRestarted { id: self.spec.id })
                        .await;
                }
                self.emit_published_after_reconnect(refreshed).await;
            }
            Err(error) => {
                if error.is_websocket_unauthorized() {
                    self.force_refresh_token = Some(access_token);
                }
                self.reconnect_backoff.record_failure();
                self.emit_reconnecting_state().await;
                emit_log_error(
                    self.event_sink.as_ref(),
                    self.spec.id,
                    format!("host relay reconnect failed: {error}"),
                )
                .await;
            }
        }
    }

    async fn emit_published_after_reconnect(&self, refreshed: bool) {
        emit_state(
            self.event_sink.as_ref(),
            self.spec.id,
            SessionState::Published,
        )
        .await;
        let message = if refreshed {
            "host relay refreshed sign-in and reconnected"
        } else {
            "host relay reconnected"
        };
        emit_info(self.event_sink.as_ref(), self.spec.id, message.to_owned()).await;
    }

    async fn record_authentication_required(&mut self, reason: String) {
        self.reconnect_backoff.record_failure();
        self.emit_reconnecting_state().await;
        if !self.awaiting_reauthentication {
            emit_backend_access_invalid(self.event_sink.as_ref(), reason.clone()).await;
            emit_info(
                self.event_sink.as_ref(),
                self.spec.id,
                format!("host relay is waiting for sign-in: {reason}"),
            )
            .await;
            self.awaiting_reauthentication = true;
        }
    }

    async fn record_auth_temporarily_unavailable(&mut self, reason: String) {
        self.reconnect_backoff.record_failure();
        self.emit_reconnecting_state().await;
        emit_info(
            self.event_sink.as_ref(),
            self.spec.id,
            format!("host relay is waiting for auth refresh: {reason}"),
        )
        .await;
    }

    async fn emit_reconnecting_state(&self) {
        emit_state(
            self.event_sink.as_ref(),
            self.spec.id,
            SessionState::Reconnecting,
        )
        .await;
    }
}
