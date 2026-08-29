use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

use kodosi_domain::{
    ids::SessionId,
    lifecycle::{
        ConnectionState, RemoteActionStatus, RemoteSessionAccessIssue, RemoteSessionAccessState,
    },
    permissions::AccessLevel,
    session::SessionState,
    terminal::{TerminalCheckpointV2, TerminalPresentationV2},
};

#[derive(Debug, Clone)]
pub enum SessionRelayEvent {
    RemoteCheckpoint {
        id: SessionId,
        next_sequence: u64,
        checkpoint: TerminalCheckpointV2,
        application: ApplicationAck,
    },
    RemoteRawBatch {
        id: SessionId,
        first_sequence: u64,
        next_sequence: u64,
        chunks: Vec<Vec<u8>>,
    },
    RemotePlainPresentation {
        id: SessionId,
        presentation: TerminalPresentationV2,
    },
    RemotePendingPermissionsSnapshot {
        id: SessionId,
        incarnation_id: uuid::Uuid,
        generation: u64,
        snapshot: serde_json::Value,
    },
    RemoteStateChanged {
        id: SessionId,
        state: SessionState,
    },
    RemoteAccessChanged {
        id: SessionId,
        access: AccessLevel,
    },
    RemoteActionResult {
        id: SessionId,
        action_id: String,
        request_id: Option<String>,
        request_generation: Option<u64>,
        status: RemoteActionStatus,
    },
    RemoteSessionConnectionChanged {
        id: SessionId,
        status: ConnectionState,
        reason: Option<String>,
    },
    RemoteAccessStateChanged {
        id: SessionId,
        state: RemoteSessionAccessState,
        reason: Option<String>,
        issue: Option<RemoteSessionAccessIssue>,
    },
    RemoteAccessRevoked {
        id: SessionId,
    },
    RemoteSessionRelayExited {
        id: SessionId,
    },
    Info {
        id: SessionId,
        message: String,
    },
    LogError {
        id: SessionId,
        message: String,
    },
    BackendAccessInvalid {
        reason: String,
    },

    RemoteSemanticReceipt {
        id: SessionId,
        receipt: super::wire::ParticipantSemanticReceiptMessage,
        persistence: ApplicationAck,
    },
    RemoteControlTrustEstablished {
        id: SessionId,
        incarnation_id: uuid::Uuid,
        owner_user_id: String,
        signer_device_id: String,
        signer_public_key: Vec<u8>,
        device_list_generation: u64,
        identity_fingerprint: [u8; 32],
    },
}

#[derive(Debug, Clone, Default)]
pub struct ApplicationAck {
    state: Arc<AtomicU8>,
    notify: Arc<Notify>,
}

impl ApplicationAck {
    pub fn complete(&self, accepted: bool) {
        let state = if accepted { 1 } else { 2 };
        if self
            .state
            .compare_exchange(0, state, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.notify.notify_one();
        }
    }

    async fn wait(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                1 => return true,
                2 => return false,
                _ => self.notify.notified().await,
            }
        }
    }
}

#[derive(Clone)]
pub struct SessionRelayEventSink {
    session_events: mpsc::Sender<SessionRelayEvent>,
    id: SessionId,
    cancellation: CancellationToken,
}

impl SessionRelayEventSink {
    pub fn new(
        session_events: mpsc::Sender<SessionRelayEvent>,
        id: SessionId,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            session_events,
            id,
            cancellation,
        }
    }

    pub const fn id(&self) -> SessionId {
        self.id
    }

    async fn send(&self, event: SessionRelayEvent) {
        drop(self.session_events.send(event).await);
    }

    pub async fn checkpoint(&self, next_sequence: u64, checkpoint: TerminalCheckpointV2) -> bool {
        let application = ApplicationAck::default();
        if self
            .session_events
            .send(SessionRelayEvent::RemoteCheckpoint {
                id: self.id,
                next_sequence,
                checkpoint,
                application: application.clone(),
            })
            .await
            .is_err()
        {
            return false;
        }
        self.wait_for_ack(&application).await
    }

    pub async fn raw_batch(&self, first_sequence: u64, next_sequence: u64, chunks: Vec<Vec<u8>>) {
        self.send(SessionRelayEvent::RemoteRawBatch {
            id: self.id,
            first_sequence,
            next_sequence,
            chunks,
        })
        .await;
    }

    pub async fn plain_presentation(&self, presentation: TerminalPresentationV2) {
        self.send(SessionRelayEvent::RemotePlainPresentation {
            id: self.id,
            presentation,
        })
        .await;
    }

    pub async fn pending_permissions_snapshot(
        &self,
        incarnation_id: uuid::Uuid,
        generation: u64,
        snapshot: serde_json::Value,
    ) {
        self.send(SessionRelayEvent::RemotePendingPermissionsSnapshot {
            id: self.id,
            incarnation_id,
            generation,
            snapshot,
        })
        .await;
    }

    async fn wait_for_ack(&self, ack: &ApplicationAck) -> bool {
        tokio::select! {
            accepted = ack.wait() => accepted,
            () = self.cancellation.cancelled() => false,
        }
    }

    pub async fn remote_control_trust(
        &self,
        incarnation_id: uuid::Uuid,
        trust: &crate::session_key_service::SessionKeyTrustedSigner,
    ) {
        self.send(SessionRelayEvent::RemoteControlTrustEstablished {
            id: self.id,
            incarnation_id,
            owner_user_id: trust.owner_user_id.clone(),
            signer_device_id: trust.sender_device_id.clone(),
            signer_public_key: trust.public_key.clone(),
            device_list_generation: trust.device_list_generation,
            identity_fingerprint: trust.identity_fingerprint,
        })
        .await;
    }

    pub async fn semantic_receipt(
        &self,
        receipt: super::wire::ParticipantSemanticReceiptMessage,
    ) -> bool {
        let persistence = ApplicationAck::default();
        if self
            .session_events
            .send(SessionRelayEvent::RemoteSemanticReceipt {
                id: self.id,
                receipt,
                persistence: persistence.clone(),
            })
            .await
            .is_err()
        {
            return false;
        }
        self.wait_for_ack(&persistence).await
    }

    pub async fn state(&self, state: SessionState) {
        self.send(SessionRelayEvent::RemoteStateChanged { id: self.id, state })
            .await;
    }

    pub async fn access(&self, access: AccessLevel) {
        self.send(SessionRelayEvent::RemoteAccessChanged {
            id: self.id,
            access,
        })
        .await;
    }

    pub async fn action_result(
        &self,
        action_id: String,
        request_id: Option<String>,
        request_generation: Option<u64>,
        status: RemoteActionStatus,
    ) {
        self.send(SessionRelayEvent::RemoteActionResult {
            id: self.id,
            action_id,
            request_id,
            request_generation,
            status,
        })
        .await;
    }

    pub async fn connection(&self, status: ConnectionState, reason: Option<String>) {
        self.send(SessionRelayEvent::RemoteSessionConnectionChanged {
            id: self.id,
            status,
            reason,
        })
        .await;
    }

    pub async fn access_state(
        &self,
        state: RemoteSessionAccessState,
        reason: Option<String>,
        issue: Option<RemoteSessionAccessIssue>,
    ) {
        self.send(SessionRelayEvent::RemoteAccessStateChanged {
            id: self.id,
            state,
            reason,
            issue,
        })
        .await;
    }

    pub async fn access_revoked(&self) {
        self.send(SessionRelayEvent::RemoteAccessRevoked { id: self.id })
            .await;
    }

    pub async fn exited(&self) {
        self.send(SessionRelayEvent::RemoteSessionRelayExited { id: self.id })
            .await;
    }

    pub async fn info(&self, message: String) {
        self.send(SessionRelayEvent::Info {
            id: self.id,
            message,
        })
        .await;
    }

    pub async fn log_error(&self, message: String) {
        self.send(SessionRelayEvent::LogError {
            id: self.id,
            message,
        })
        .await;
    }

    pub async fn backend_access_invalid(&self, reason: String) {
        self.send(SessionRelayEvent::BackendAccessInvalid { reason })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::ApplicationAck;

    #[tokio::test]
    async fn verification_completion_before_wait_is_retained() {
        let ack = ApplicationAck::default();
        ack.complete(true);

        assert!(
            tokio::time::timeout(Duration::from_millis(50), ack.wait())
                .await
                .expect("stored completion should not wait")
        );
    }

    #[tokio::test]
    async fn verification_waiter_is_woken_once() {
        let ack = ApplicationAck::default();
        let waiting = {
            let ack = ack.clone();
            tokio::spawn(async move { ack.wait().await })
        };
        tokio::task::yield_now().await;
        ack.complete(false);

        assert!(
            !tokio::time::timeout(Duration::from_millis(50), waiting)
                .await
                .expect("waiter should wake")
                .expect("waiter should not panic")
        );
    }
}
