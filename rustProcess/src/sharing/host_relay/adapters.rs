use std::sync::Arc;

use tokio::sync::mpsc;

use crate::local_sessions::size_authority::{SizeAuthorityCell, SizeOrigin};
use crate::session_runtime::{
    commands::SessionInput,
    events::{AccountEventOrigin, HostRelayEventOrigin, RuntimeSessionEvent},
    handles::{SessionPtyInstruction, SessionRuntimeHandle, SessionScreenInstruction},
};
use kodosi_backend_client::relay::{
    ClientFocus, HostRelayEvent, HostRelayEventFuture, HostRelayEventSink, HostRelayFuture,
    HostRelayPort, HostRelayPortError, HostRelayTerminalCheckpoint, HostRelayTerminalPresentation,
};
use kodosi_domain::terminal::{TerminalPixelGeometry, TerminalSize};

#[derive(Debug)]
pub(crate) struct KodosiHostRelayPort {
    runtime: SessionRuntimeHandle,

    size_authority: Arc<SizeAuthorityCell>,
}

impl KodosiHostRelayPort {
    pub(crate) const fn new(
        runtime: SessionRuntimeHandle,
        size_authority: Arc<SizeAuthorityCell>,
    ) -> Self {
        Self {
            runtime,
            size_authority,
        }
    }
}

impl HostRelayPort for KodosiHostRelayPort {
    fn dispatch_input_payload<'a>(
        &'a self,
        payload: &'a [u8],
        owner_origin: bool,
    ) -> HostRelayFuture<'a, ()> {
        Box::pin(async move {
            let inputs = parse_input_payload(payload);
            let mut claim =
                if owner_origin && inputs.iter().any(SizeAuthorityCell::input_claims_authority) {
                    Some(self.size_authority.begin_claim(SizeOrigin::Remote).await)
                } else {
                    None
                };
            let reclaim_size = claim.as_ref().and_then(|claim| match claim.preview() {
                crate::local_sessions::size_authority::ClaimPreview::Reclaim(size) => size,
                crate::local_sessions::size_authority::ClaimPreview::Unavailable
                | crate::local_sessions::size_authority::ClaimPreview::AlreadyHeld => None,
            });
            self.runtime
                .send_inputs_with_reclaim(inputs, reclaim_size)
                .await
                .map_err(HostRelayPortError::from_error)?;
            if let Some(claim) = claim.take() {
                claim.commit();
            }
            drop(claim);
            Ok(())
        })
    }

    fn stop(&self) -> HostRelayFuture<'_, ()> {
        Box::pin(async move {
            self.runtime
                .send_pty_instruction(SessionPtyInstruction::Kill)
                .await
                .map_err(HostRelayPortError::from_error)
        })
    }

    fn interrupt(&self) -> HostRelayFuture<'_, ()> {
        Box::pin(async move {
            self.runtime
                .send_pty_instruction(SessionPtyInstruction::Interrupt)
                .await
                .map_err(HostRelayPortError::from_error)
        })
    }

    fn resize(
        &self,
        size: TerminalSize,
        pixel_geometry: Option<TerminalPixelGeometry>,
        owner_origin: bool,
        claim: bool,
    ) -> HostRelayFuture<'_, ()> {
        Box::pin(async move {
            if !owner_origin {
                tracing::debug!(
                    rows = size.rows(),
                    cols = size.cols(),
                    "dropped shared-participant resize: only owner surfaces size the PTY"
                );
                return Ok(());
            }
            let mut claim = if claim {
                Some(self.size_authority.begin_claim(SizeOrigin::Remote).await)
            } else {
                None
            };
            if claim.is_none() && !self.size_authority.admit_resize(SizeOrigin::Remote, size) {
                tracing::debug!(
                    rows = size.rows(),
                    cols = size.cols(),
                    "dropped remote resize: local surface holds size authority"
                );
                return Ok(());
            }
            self.runtime
                .resize_and_wait(size, pixel_geometry)
                .await
                .map_err(HostRelayPortError::from_error)?;
            if let Some(claim) = claim.take() {
                let _ = self.size_authority.admit_resize(SizeOrigin::Remote, size);
                claim.commit();
            }
            drop(claim);
            Ok(())
        })
    }

    fn set_focus(&self, client_id: String, focus: ClientFocus) -> HostRelayFuture<'_, ()> {
        Box::pin(async move {
            let instruction = match focus {
                ClientFocus::Focused => SessionScreenInstruction::Focus { client_id },
                ClientFocus::Blurred => SessionScreenInstruction::Blur { client_id },
            };
            self.runtime
                .send_screen_instruction(instruction)
                .await
                .map_err(HostRelayPortError::from_error)
        })
    }

    fn capture_terminal_checkpoint(&self) -> HostRelayFuture<'_, HostRelayTerminalCheckpoint> {
        Box::pin(async move {
            let capture = self
                .runtime
                .capture_checkpoint_data()
                .await
                .map_err(HostRelayPortError::from_error)?;
            Ok(HostRelayTerminalCheckpoint {
                checkpoint: capture.checkpoint,
                next_sequence: capture.applied_sequence,
            })
        })
    }

    fn capture_terminal_presentation(&self) -> HostRelayFuture<'_, HostRelayTerminalPresentation> {
        Box::pin(async move {
            let capture = self
                .runtime
                .capture_presentation_data()
                .await
                .map_err(HostRelayPortError::from_error)?;
            Ok(HostRelayTerminalPresentation {
                presentation: capture.presentation,
            })
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeHostRelayEventSink {
    session_events: mpsc::Sender<RuntimeSessionEvent>,
    origin: HostRelayEventOrigin,
}

impl RuntimeHostRelayEventSink {
    pub(crate) const fn new(
        session_events: mpsc::Sender<RuntimeSessionEvent>,
        session_id: kodosi_domain::ids::SessionId,
        relay_generation: u64,
        account_origin: AccountEventOrigin,
    ) -> Self {
        Self {
            session_events,
            origin: HostRelayEventOrigin {
                account_origin,
                session_id,
                relay_generation,
            },
        }
    }
}

impl HostRelayEventSink for RuntimeHostRelayEventSink {
    fn emit(&self, event: HostRelayEvent) -> HostRelayEventFuture<'_> {
        Box::pin(async move {
            if let Some(event) = map_host_relay_event(event, &self.origin) {
                drop(self.session_events.send(event).await);
            }
        })
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive relay-event match makes provenance and cross-session rejection auditable for every source variant"
)]
fn map_host_relay_event(
    event: HostRelayEvent,
    origin: &HostRelayEventOrigin,
) -> Option<RuntimeSessionEvent> {
    let mapped = match event {
        HostRelayEvent::Info { id, message } => {
            (id == origin.session_id).then(|| RuntimeSessionEvent::HostRelayInfo {
                origin: origin.clone(),
                message,
            })
        }
        HostRelayEvent::LogError { id, message } => {
            (id == origin.session_id).then(|| RuntimeSessionEvent::HostRelayLogError {
                origin: origin.clone(),
                message,
            })
        }
        HostRelayEvent::BackendAccessInvalid { reason } => {
            Some(RuntimeSessionEvent::HostRelayBackendAccessInvalid {
                origin: origin.clone(),
                reason,
            })
        }
        HostRelayEvent::HostDemand {
            id,
            required,
            participant_count,
            reason,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::HostDemand {
            origin: origin.clone(),
            required,
            participant_count,
            reason,
        }),
        HostRelayEvent::StateChanged { id, state } => {
            (id == origin.session_id).then(|| RuntimeSessionEvent::StateChanged {
                origin: origin.clone(),
                state,
            })
        }
        HostRelayEvent::HostAccessRevoked {
            id,
            revoked_user_id,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::HostAccessRevoked {
            origin: origin.clone(),
            revoked_user_id,
        }),
        HostRelayEvent::BackendRelayRestarted { id } => {
            (id == origin.session_id).then(|| RuntimeSessionEvent::HostRelayBackendRestarted {
                origin: origin.clone(),
            })
        }
        HostRelayEvent::HostFrameReservationExhausted { id } => {
            (id == origin.session_id).then(|| RuntimeSessionEvent::HostFrameReservationExhausted {
                origin: origin.clone(),
            })
        }
        HostRelayEvent::HostKeyDistributionRequested { id, fence_id } => (id == origin.session_id)
            .then(|| RuntimeSessionEvent::HostKeyDistributionRequested {
                origin: origin.clone(),
                fence_id,
            }),
        HostRelayEvent::HostKeyRotationRequired {
            id,
            reason,
            fail_closed,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::HostKeyRotationRequired {
            origin: origin.clone(),
            reason,
            fail_closed,
        }),
        HostRelayEvent::ActionCompleted {
            id,
            incarnation_id,
            action_id,
            request_id,
            requester_user_id,
            requester_device_id,
            accepted,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::RemoteActionCompleted {
            origin: origin.clone(),
            incarnation_id,
            action_id,
            request_id,
            requester_user_id,
            requester_device_id,
            accepted,
        }),
        HostRelayEvent::PermissionDecision {
            id,
            incarnation_id,
            action_id,
            request_id,
            request_generation,
            decision,
            decider_user_id,
            decider_device_id,
            reply,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::RemotePermissionDecision {
            origin: origin.clone(),
            incarnation_id,
            action_id,
            request_id,
            request_generation,
            decision,
            decider_user_id,
            decider_device_id,
            reply,
        }),
        HostRelayEvent::SemanticSend {
            id,
            request_id,
            incarnation_id,
            mode,
            payload_sha256,
            text,
            requester_user_id,
            requester_device_id,
            reply,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::HostSemanticSend {
            origin: origin.clone(),
            request_id,
            incarnation_id,
            mode,
            payload_sha256,
            text,
            requester_user_id,
            requester_device_id,
            reply,
        }),
        HostRelayEvent::SemanticCancel {
            id,
            request_id,
            incarnation_id,
            mode,
            payload_sha256,
            requester_user_id,
            requester_device_id,
            reply,
        } => (id == origin.session_id).then(|| RuntimeSessionEvent::HostSemanticCancel {
            origin: origin.clone(),
            request_id,
            incarnation_id,
            mode,
            payload_sha256,
            requester_user_id,
            requester_device_id,
            reply,
        }),
    };
    if mapped.is_none() {
        tracing::warn!(
            authoritative_session_id = %origin.session_id,
            "dropped host relay event targeting a different session"
        );
    }
    mapped
}

fn parse_input_payload(payload: &[u8]) -> Vec<SessionInput> {
    vec![SessionInput::new(payload.to_vec())]
}

#[cfg(test)]
mod tests;
