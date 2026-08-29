pub mod emit;
mod events;
mod lifecycle;
mod lifecycle_reconnect;
mod owner_messages;
mod port;
mod transport;
mod wire;

use std::{fmt, future::Future, sync::Arc, time::Duration};

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    BackendClientError, Result,
    auth::{BackendAccessTokenState, BackendAuthProvider},
    crypto,
    host_ws::HostWsClient,
};
use kodosi_domain::ids::SessionId;

pub use self::events::{
    HostRelayEvent, HostRelayEventFuture, HostRelayEventSink, SemanticAdmissionReply,
};
pub use self::port::{
    ClientFocus, HostRelayFuture, HostRelayPort, HostRelayPortError, HostRelayTerminalCheckpoint,
    HostRelayTerminalPresentation,
};
pub use self::transport::{
    CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES, EncryptionState,
    PENDING_PERMISSIONS_ENCRYPTED_FRAME_MAX_BYTES, PENDING_PERMISSIONS_FRAME_TYPE,
    build_checkpoint_frame, build_pending_permissions_frame, build_presentation_frame,
    build_raw_batch_frame, raw_batch_encrypted_frame_max_bytes,
};

use self::{emit::emit_backend_access_invalid, lifecycle::run_loop, wire::connect_and_prime};

const RELAY_AUTH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostIncomingMessageType {
    Accepted,
    StreamDemand,
    KeyDistributionRequested,
    SemanticSend,
    SemanticCancel,
    ActionResultAck,
    SemanticReceiptAck,
    Stop,
    Interrupt,
    Input,
    Resize,
    FocusChanged,
    ParticipantDisconnected,
    Suggestion,
    ParticipantChanged,
    AccessRevoked,
    PermissionDecision,
}

impl HostIncomingMessageType {
    pub const ALL: &'static [&'static str] = &[
        "host.accepted",
        "host.streamDemand",
        "host.keyDistributionRequested",
        "host.semanticSend",
        "host.semanticCancel",
        "host.actionResultAck",
        "host.semanticReceiptAck",
        "host.stop",
        "host.interrupt",
        "host.input",
        "host.resize",
        "host.focusChanged",
        "host.participantDisconnected",
        "host.suggestion",
        "host.participantChanged",
        "host.accessRevoked",
        "host.permissionDecision",
    ];

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "host.accepted" => Self::Accepted,
            "host.streamDemand" => Self::StreamDemand,
            "host.keyDistributionRequested" => Self::KeyDistributionRequested,
            "host.semanticSend" => Self::SemanticSend,
            "host.semanticCancel" => Self::SemanticCancel,
            "host.actionResultAck" => Self::ActionResultAck,
            "host.semanticReceiptAck" => Self::SemanticReceiptAck,
            "host.stop" => Self::Stop,
            "host.interrupt" => Self::Interrupt,
            "host.input" => Self::Input,
            "host.resize" => Self::Resize,
            "host.focusChanged" => Self::FocusChanged,
            "host.participantDisconnected" => Self::ParticipantDisconnected,
            "host.suggestion" => Self::Suggestion,
            "host.participantChanged" => Self::ParticipantChanged,
            "host.accessRevoked" => Self::AccessRevoked,
            "host.permissionDecision" => Self::PermissionDecision,
            _ => return None,
        })
    }
}

pub(super) async fn await_relay_operation<T>(
    operation: &'static str,
    timeout: Duration,
    cancellation: &CancellationToken,
    future: impl Future<Output = T>,
) -> Result<T> {
    tokio::select! {
        () = cancellation.cancelled() => Err(BackendClientError::Unsupported {
            reason: format!("{operation} cancelled"),
        }),
        result = tokio::time::timeout(timeout, future) => result.map_err(|_| {
            BackendClientError::Timeout { operation }
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRelayTerminalEvent {
    Raw { sequence: u64, bytes: Bytes },
    ForceCheckpoint,
    ForcePresentation,
    Closed { final_sequence: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRelayPendingPermissionsSnapshot {
    pub generation: u64,
    pub incarnation_id: uuid::Uuid,
    pub plaintext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRelaySemanticReceipt {
    pub request_id: uuid::Uuid,
    pub incarnation_id: uuid::Uuid,
    pub mode: crate::session_relay::wire::RelaySemanticMode,
    pub payload_sha256: String,
    pub outcome: crate::session_relay::wire::RelaySemanticOutcome,
    pub requester_user_id: String,
    pub requester_device_id: String,
    pub owner_user_id: String,
    pub owner_device_id: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRelayActionResult {
    pub incarnation_id: uuid::Uuid,
    pub action_id: String,
    pub request_id: String,
    pub request_generation: u64,
    pub requester_user_id: String,
    pub requester_device_id: String,
    pub accepted: bool,
}

#[derive(Debug)]
pub struct HostRelayFenceCompletion {
    pub fence_id: String,
}

#[derive(Debug)]
pub struct HostRelayActionResultDelivery {
    pub result: HostRelayActionResult,
    pub delivery: tokio::sync::oneshot::Sender<bool>,
}

pub fn build_semantic_receipt_dto(
    backend_session_id: &str,
    receipt_signing_pkcs8: &[u8],
    receipt: &HostRelaySemanticReceipt,
) -> Result<crate::dto::semantic_receipts::SemanticReceiptDto> {
    wire::build_semantic_receipt_dto(backend_session_id, receipt_signing_pkcs8, receipt)
}

#[derive(Debug)]
pub struct HostRelaySemanticReceiptDelivery {
    pub receipt: HostRelaySemanticReceipt,
    pub delivery: tokio::sync::oneshot::Sender<bool>,
}

pub struct HostRelaySpec {
    pub id: SessionId,
    pub backend_session_id: String,
    pub backend_incarnation_id: uuid::Uuid,
    pub owner_secret: zeroize::Zeroizing<String>,
    pub port: Arc<dyn HostRelayPort>,
    pub frame_revision_start: u64,
    pub frame_revision_end_exclusive: u64,
    pub frame_key_generation: u32,
    pub frame_nonce_start: u64,
    pub frame_nonce_end_exclusive: u64,
    pub session_key: zeroize::Zeroizing<crypto::SessionKey>,
    pub owner_user_id: String,
    pub control_trust: crate::control::ControlTrustStore,
    pub host_device_id: String,
    pub host_signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
}

impl fmt::Debug for HostRelaySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostRelaySpec")
            .field("id", &self.id)
            .field("backend_session_id", &self.backend_session_id)
            .field("backend_incarnation_id", &self.backend_incarnation_id)
            .field("owner_secret", &"<redacted>")
            .field("port", &self.port)
            .field("frame_revision_start", &self.frame_revision_start)
            .field(
                "frame_revision_end_exclusive",
                &self.frame_revision_end_exclusive,
            )
            .field("frame_key_generation", &self.frame_key_generation)
            .field("frame_nonce_start", &self.frame_nonce_start)
            .field("frame_nonce_end_exclusive", &self.frame_nonce_end_exclusive)
            .field("session_key", &"<redacted>")
            .field("owner_user_id", &self.owner_user_id)
            .field("control_trust", &self.control_trust)
            .field("host_device_id", &self.host_device_id)
            .field("host_signing_pkcs8", &"<redacted>")
            .finish()
    }
}

#[derive(Debug)]
struct PreparedRelayCancellation {
    token: CancellationToken,
    armed: bool,
}

impl Drop for PreparedRelayCancellation {
    fn drop(&mut self) {
        if self.armed {
            self.token.cancel();
        }
    }
}

pub struct PreparedHostRelay {
    spec: HostRelaySpec,
    terminal_rx: mpsc::Receiver<HostRelayTerminalEvent>,
    host_ws: HostWsClient,
    auth_provider: BackendAuthProvider,
    event_sink: Arc<dyn HostRelayEventSink>,
    pending_permissions_rx:
        tokio::sync::watch::Receiver<Option<HostRelayPendingPermissionsSnapshot>>,
    semantic_receipt_rx: mpsc::Receiver<HostRelaySemanticReceiptDelivery>,
    action_result_rx: mpsc::Receiver<HostRelayActionResultDelivery>,
    fence_completion_rx: mpsc::Receiver<HostRelayFenceCompletion>,
    cancellation: PreparedRelayCancellation,
    primed: wire::PrimedHostConnection,
    access_token: zeroize::Zeroizing<String>,
    initial_relay_epoch: String,
    encryption: EncryptionState,
}

impl fmt::Debug for PreparedHostRelay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedHostRelay")
            .field("session_id", &self.spec.id)
            .field("backend_session_id", &self.spec.backend_session_id)
            .finish_non_exhaustive()
    }
}

pub struct HostRelayHandle {
    pub cancellation: CancellationToken,
    pub join_handle: tokio::task::JoinHandle<()>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "relay preparation takes one explicit bounded receiver per durable lane"
)]
#[tracing::instrument(skip_all, fields(session_id = %spec.id, backend_session_id = %spec.backend_session_id), err)]
pub async fn prepare(
    spec: HostRelaySpec,
    terminal_rx: mpsc::Receiver<HostRelayTerminalEvent>,
    host_ws: HostWsClient,
    auth_provider: BackendAuthProvider,
    event_sink: Arc<dyn HostRelayEventSink>,
    pending_permissions_rx: tokio::sync::watch::Receiver<
        Option<HostRelayPendingPermissionsSnapshot>,
    >,
    semantic_receipt_rx: mpsc::Receiver<HostRelaySemanticReceiptDelivery>,
    action_result_rx: mpsc::Receiver<HostRelayActionResultDelivery>,
    fence_completion_rx: mpsc::Receiver<HostRelayFenceCompletion>,
    cancellation: CancellationToken,
) -> Result<PreparedHostRelay> {
    let encryption = EncryptionState::derive(
        &spec.session_key,
        spec.frame_key_generation,
        spec.frame_nonce_start,
        spec.frame_nonce_end_exclusive,
    )?;
    let mut rejected_token: Option<zeroize::Zeroizing<String>> = None;
    let (access_token, primed) = loop {
        let token_state = await_relay_operation(
            "host relay authentication",
            RELAY_AUTH_TIMEOUT,
            &cancellation,
            auth_provider.access_token(rejected_token.as_deref().map(ToOwned::to_owned)),
        )
        .await??;
        let access_token = match token_state {
            BackendAccessTokenState::Ready { access_token, .. } => access_token,
            BackendAccessTokenState::RequiresLogin { reason } => {
                emit_backend_access_invalid(event_sink.as_ref(), reason.to_string()).await;
                return Err(BackendClientError::Unauthorized);
            }
            BackendAccessTokenState::TemporarilyUnavailable { reason } => {
                return Err(BackendClientError::Unsupported {
                    reason: reason.to_string(),
                });
            }
        };
        match Box::pin(connect_and_prime(
            &spec,
            &host_ws,
            &access_token,
            &cancellation,
        ))
        .await
        {
            Ok(primed) => break (access_token, primed),
            Err(error) if error.is_websocket_unauthorized() && rejected_token.is_none() => {
                rejected_token = Some(access_token);
            }
            Err(error) => return Err(error),
        }
    };
    let initial_relay_epoch = primed.accepted.relay_epoch.clone();

    Ok(PreparedHostRelay {
        spec,
        terminal_rx,
        host_ws,
        auth_provider,
        event_sink,
        pending_permissions_rx,
        semantic_receipt_rx,
        action_result_rx,
        fence_completion_rx,
        cancellation: PreparedRelayCancellation {
            token: cancellation,
            armed: true,
        },
        primed,
        access_token,
        initial_relay_epoch,
        encryption,
    })
}

impl PreparedHostRelay {
    pub fn activate(mut self) -> HostRelayHandle {
        let cancellation = self.cancellation.token.clone();
        let task_cancellation = cancellation.clone();
        self.cancellation.armed = false;
        let join_handle = tokio::spawn(async move {
            Box::pin(run_loop(
                self.spec,
                self.terminal_rx,
                self.host_ws,
                self.auth_provider,
                self.event_sink,
                self.pending_permissions_rx,
                self.semantic_receipt_rx,
                self.action_result_rx,
                self.fence_completion_rx,
                task_cancellation,
                self.primed.stream,
                self.access_token,
                self.initial_relay_epoch,
                self.encryption,
            ))
            .await;
        });
        HostRelayHandle {
            cancellation,
            join_handle,
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::{PreparedRelayCancellation, await_relay_operation};
    use crate::BackendClientError;

    #[test]
    fn prepared_relay_cancellation_is_raii_and_disarmable() {
        let dropped = CancellationToken::new();
        drop(PreparedRelayCancellation {
            token: dropped.clone(),
            armed: true,
        });
        assert!(dropped.is_cancelled());

        let activated = CancellationToken::new();
        drop(PreparedRelayCancellation {
            token: activated.clone(),
            armed: false,
        });
        assert!(!activated.is_cancelled());
    }

    #[tokio::test]
    async fn relay_operation_timeout_is_bounded() {
        let result = await_relay_operation(
            "test operation",
            Duration::from_millis(1),
            &CancellationToken::new(),
            std::future::pending::<()>(),
        )
        .await;

        std::assert_matches!(
            result,
            Err(BackendClientError::Timeout {
                operation: "test operation"
            })
        );
    }

    #[tokio::test]
    async fn relay_operation_observes_cancellation() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let result = await_relay_operation(
            "test operation",
            Duration::from_mins(1),
            &cancellation,
            std::future::pending::<()>(),
        )
        .await;

        std::assert_matches!(
            result,
            Err(BackendClientError::Unsupported { reason })
                if reason == "test operation cancelled"
        );
    }
}
