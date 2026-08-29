use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use kodosi_domain::{ids::SessionId, session::SessionState};

pub type HostRelayEventFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

pub trait HostRelayEventSink: fmt::Debug + Send + Sync {
    fn emit(&self, event: HostRelayEvent) -> HostRelayEventFuture<'_>;
}

#[derive(Debug, Clone)]
pub struct SemanticAdmissionReply(Arc<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>);

impl SemanticAdmissionReply {
    pub fn channel() -> (Self, tokio::sync::oneshot::Receiver<bool>) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        (Self(Arc::new(Mutex::new(Some(sender)))), receiver)
    }

    pub fn complete(&self, accepted: bool) {
        if let Ok(mut sender) = self.0.lock()
            && let Some(sender) = sender.take()
        {
            let _ignored = sender.send(accepted);
        }
    }
}

#[derive(Debug, Clone)]
pub enum HostRelayEvent {
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
    HostDemand {
        id: SessionId,
        required: bool,
        participant_count: usize,
        reason: String,
    },
    StateChanged {
        id: SessionId,
        state: SessionState,
    },
    HostAccessRevoked {
        id: SessionId,
        revoked_user_id: String,
    },
    BackendRelayRestarted {
        id: SessionId,
    },
    HostFrameReservationExhausted {
        id: SessionId,
    },
    HostKeyDistributionRequested {
        id: SessionId,
        fence_id: String,
    },

    HostKeyRotationRequired {
        id: SessionId,
        reason: String,
        fail_closed: bool,
    },
    ActionCompleted {
        id: SessionId,
        incarnation_id: uuid::Uuid,
        action_id: String,
        request_id: String,
        requester_user_id: String,
        requester_device_id: String,
        accepted: bool,
    },

    PermissionDecision {
        id: SessionId,
        incarnation_id: uuid::Uuid,
        action_id: String,
        request_id: String,
        request_generation: u64,
        decision: String,
        decider_user_id: String,
        decider_device_id: Option<String>,
        reply: SemanticAdmissionReply,
    },
    SemanticSend {
        id: SessionId,
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        mode: crate::session_relay::wire::RelaySemanticMode,
        payload_sha256: String,
        text: String,
        requester_user_id: String,
        requester_device_id: String,
        reply: SemanticAdmissionReply,
    },
    SemanticCancel {
        id: SessionId,
        request_id: uuid::Uuid,
        incarnation_id: uuid::Uuid,
        mode: crate::session_relay::wire::RelaySemanticMode,
        payload_sha256: String,
        requester_user_id: String,
        requester_device_id: String,
        reply: SemanticAdmissionReply,
    },
}
