use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use futures_util::StreamExt;
use tokio::{sync::mpsc, time};
use tokio_tungstenite::tungstenite::{Error as WebSocketError, Message};
use tokio_util::sync::CancellationToken;

use crate::{
    auth::BackendAuthProvider,
    backoff::ReconnectBackoff,
    close_reasons::{self, CloseBehavior},
    host_ws::{HostWebSocketStream, HostWsClient},
};

use super::{
    HostRelayActionResultDelivery, HostRelayEventSink, HostRelayFenceCompletion,
    HostRelayPendingPermissionsSnapshot, HostRelaySemanticReceiptDelivery, HostRelaySpec,
    HostRelayTerminalEvent,
    owner_messages::{HostFenceDedupe, OwnerMessageOutcome, handle_owner_message},
    transport::{EncryptionState, LiveStreamState, send_checkpoint, send_presentation},
    wire::{
        HostActionResultAckMessage, HostEnvelope, HostSemanticReceiptAckMessage, send_heartbeat,
        send_host_end, send_semantic_receipt,
    },
};

const TERMINAL_CLOSE_CANCELLATION_GRACE: Duration = Duration::from_millis(250);

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const SEMANTIC_RECEIPT_ACK_TIMEOUT: Duration = Duration::from_secs(15);

const PRESENTATION_INTERVAL: Duration = Duration::from_millis(33);

#[expect(
    clippy::too_many_arguments,
    reason = "relay loop receives all lifecycle dependencies at spawn"
)]
#[tracing::instrument(skip_all, fields(session_id = %spec.id, backend_session_id = %spec.backend_session_id))]
pub(super) async fn run_loop(
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
    initial_stream: HostWebSocketStream,
    initial_access_token: zeroize::Zeroizing<String>,
    initial_relay_epoch: String,
    encryption: EncryptionState,
) {
    Box::pin(
        HostRelayLoop::new(
            spec,
            terminal_rx,
            host_ws,
            auth_provider,
            event_sink,
            pending_permissions_rx,
            semantic_receipt_rx,
            action_result_rx,
            fence_completion_rx,
            cancellation,
            initial_stream,
            initial_access_token,
            initial_relay_epoch,
            encryption,
        )
        .run(),
    )
    .await;
}

#[derive(Debug)]
pub(super) struct TerminalRelayState {
    pub checkpoint_revision: u64,
    pub presentation_revision: u64,
    pub expected_raw_sequence: Option<u64>,
    pub pending_raw: Vec<Bytes>,
    pub checkpoint_pending: bool,
    pub presentation_pending: bool,
}

pub(super) struct HostRelayLoop {
    pub spec: HostRelaySpec,
    pub terminal_rx: mpsc::Receiver<HostRelayTerminalEvent>,
    pub host_ws: HostWsClient,
    pub auth_provider: BackendAuthProvider,
    pub event_sink: Arc<dyn HostRelayEventSink>,
    pub pending_permissions_rx:
        tokio::sync::watch::Receiver<Option<HostRelayPendingPermissionsSnapshot>>,
    pub semantic_receipt_rx: mpsc::Receiver<HostRelaySemanticReceiptDelivery>,
    pub action_result_rx: mpsc::Receiver<HostRelayActionResultDelivery>,
    pub fence_completion_rx: mpsc::Receiver<HostRelayFenceCompletion>,
    pub cancellation: CancellationToken,
    pub connected: Option<HostWebSocketStream>,
    pub terminal: TerminalRelayState,
    pub reconnect_backoff: ReconnectBackoff,
    pub connection_stability_recorded: bool,
    pub live_stream: LiveStreamState,
    pub awaiting_reauthentication: bool,
    pub last_access_token: Option<zeroize::Zeroizing<String>>,
    pub force_refresh_token: Option<zeroize::Zeroizing<String>>,
    pub heartbeat: time::Interval,
    pub presentation_tick: time::Interval,
    pub pending_permissions: PendingPermissionsChannelState,
    pub encryption: EncryptionState,
    pub relay_epoch: String,
    pending_semantic_receipt: Option<PendingSemanticReceipt>,
    pending_action_result: Option<PendingActionResult>,
    fence_dedupe: HostFenceDedupe,
}

pub(super) struct PendingPermissionsChannelState {
    pub open: bool,
    pub send_required: bool,
}

struct PendingSemanticReceipt {
    receipt: super::HostRelaySemanticReceipt,
    delivery: tokio::sync::oneshot::Sender<bool>,
    deadline: time::Instant,
}

struct PendingActionResult {
    result: super::HostRelayActionResult,
    delivery: tokio::sync::oneshot::Sender<bool>,
    deadline: time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectedOutcome {
    Continue,
    Reconnect,
    Stop,
}

impl HostRelayLoop {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor mirrors run_loop parameters"
    )]
    fn new(
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
        initial_stream: HostWebSocketStream,
        initial_access_token: zeroize::Zeroizing<String>,
        initial_relay_epoch: String,
        encryption: EncryptionState,
    ) -> Self {
        let mut heartbeat = time::interval(HEARTBEAT_INTERVAL);
        let mut presentation_tick = time::interval(PRESENTATION_INTERVAL);
        heartbeat.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        presentation_tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let checkpoint_revision = spec.frame_revision_start;
        let presentation_revision = spec.frame_revision_start;
        Self {
            spec,
            terminal_rx,
            host_ws,
            auth_provider,
            event_sink,
            pending_permissions_rx,
            semantic_receipt_rx,
            action_result_rx,
            fence_completion_rx,
            cancellation,
            connected: Some(initial_stream),
            terminal: TerminalRelayState {
                checkpoint_revision,
                presentation_revision,
                expected_raw_sequence: None,
                pending_raw: Vec::new(),
                checkpoint_pending: false,
                presentation_pending: false,
            },
            reconnect_backoff: ReconnectBackoff::standard(),
            connection_stability_recorded: false,
            live_stream: LiveStreamState::default(),
            awaiting_reauthentication: false,
            last_access_token: Some(initial_access_token),
            force_refresh_token: None,
            heartbeat,
            presentation_tick,
            pending_permissions: PendingPermissionsChannelState {
                open: true,
                send_required: true,
            },
            encryption,
            relay_epoch: initial_relay_epoch,
            pending_semantic_receipt: None,
            pending_action_result: None,
            fence_dedupe: HostFenceDedupe::default(),
        }
    }

    async fn run(mut self) {
        loop {
            if self.connected.is_some() {
                match self.run_connected_once().await {
                    ConnectedOutcome::Continue | ConnectedOutcome::Reconnect => {}
                    ConnectedOutcome::Stop => break,
                }
            } else if !Box::pin(self.reconnect_after_backoff()).await {
                break;
            }
        }
    }

    async fn run_connected_once(&mut self) -> ConnectedOutcome {
        let Some(mut stream) = self.connected.take() else {
            return ConnectedOutcome::Reconnect;
        };
        if !self.connection_stability_recorded {
            self.reconnect_backoff.record_connected();
            self.connection_stability_recorded = true;
        }
        let outcome = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => self.handle_cancelled(&mut stream).await,
            () = async {}, if self.terminal.checkpoint_pending && self.live_stream.required => {
                self.flush_checkpoint(&mut stream).await
            },
            () = async {}, if self.pending_permissions.send_required => {
                self.flush_pending_permissions(&mut stream).await
            },
            _ = self.heartbeat.tick() => self.handle_heartbeat(&mut stream).await,
            maybe_event = self.terminal_rx.recv() => {
                self.handle_terminal_event(maybe_event, &mut stream).await
            }
            _ = self.presentation_tick.tick(), if self.terminal.presentation_pending => {
                self.handle_presentation_tick(&mut stream).await
            }
            maybe_receipt = self.semantic_receipt_rx.recv(),
                if self.pending_semantic_receipt.is_none() => {
                self.handle_semantic_receipt(maybe_receipt, &mut stream).await
            }
            maybe_result = self.action_result_rx.recv(),
                if self.pending_action_result.is_none() => {
                self.handle_action_result(maybe_result, &mut stream).await
            }
            changed = self.pending_permissions_rx.changed(),
                if self.pending_permissions.open => {
                if changed.is_ok() {
                    self.pending_permissions.send_required = true;
                } else {
                    self.pending_permissions.open = false;
                }
                ConnectedOutcome::Continue
            }
            maybe_completion = self.fence_completion_rx.recv() => {
                self.handle_fence_completion(maybe_completion, &mut stream).await
            }
            message = stream.next() => self.handle_websocket_message(message, &mut stream).await,
        };
        if outcome == ConnectedOutcome::Continue {
            self.connected = Some(stream);
        } else {
            self.reconnect_backoff.finish_connection();
            self.connection_stability_recorded = false;
        }
        outcome
    }

    async fn handle_fence_completion(
        &mut self,
        completion: Option<HostRelayFenceCompletion>,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        let Some(completion) = completion else {
            return self
                .mark_terminal_closed("host fence completion channel closed")
                .await;
        };
        self.fence_dedupe.complete(&completion.fence_id);
        match super::wire::send_fence_ack(
            stream,
            &self.spec.backend_session_id,
            &completion.fence_id,
        )
        .await
        {
            Ok(()) => ConnectedOutcome::Continue,
            Err(error) => {
                self.mark_reconnecting(format!("host fence acknowledgement failed: {error}"))
                    .await
            }
        }
    }

    async fn handle_action_result(
        &mut self,
        delivery: Option<HostRelayActionResultDelivery>,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        let Some(delivery) = delivery else {
            return self
                .mark_terminal_closed("host action-result channel closed")
                .await;
        };
        if let Err(error) = super::wire::send_host_action_result(
            stream,
            &self.spec.backend_session_id,
            &delivery.result,
        )
        .await
        {
            let _ignored = delivery.delivery.send(false);
            return self
                .mark_reconnecting(format!("host action result send failed: {error}"))
                .await;
        }
        self.pending_action_result = Some(PendingActionResult {
            result: delivery.result,
            delivery: delivery.delivery,
            deadline: time::Instant::now() + SEMANTIC_RECEIPT_ACK_TIMEOUT,
        });
        ConnectedOutcome::Continue
    }

    pub(super) fn fail_pending_action_result(&mut self) {
        if let Some(pending) = self.pending_action_result.take() {
            let _ignored = pending.delivery.send(false);
        }
    }

    async fn handle_semantic_receipt(
        &mut self,
        delivery: Option<HostRelaySemanticReceiptDelivery>,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        let Some(delivery) = delivery else {
            return self
                .mark_terminal_closed("host semantic receipt channel closed")
                .await;
        };
        if let Err(error) = send_semantic_receipt(
            stream,
            &self.spec.backend_session_id,
            &self.spec.host_signing_pkcs8,
            &delivery.receipt,
        )
        .await
        {
            let _ignored = delivery.delivery.send(false);
            return self
                .mark_reconnecting(format!("host semantic receipt send failed: {error}"))
                .await;
        }
        self.pending_semantic_receipt = Some(PendingSemanticReceipt {
            receipt: delivery.receipt,
            delivery: delivery.delivery,
            deadline: time::Instant::now() + SEMANTIC_RECEIPT_ACK_TIMEOUT,
        });
        ConnectedOutcome::Continue
    }

    pub(super) fn fail_pending_semantic_receipt(&mut self) {
        if let Some(pending) = self.pending_semantic_receipt.take() {
            let _ignored = pending.delivery.send(false);
        }
    }

    async fn handle_cancelled(&mut self, stream: &mut HostWebSocketStream) -> ConnectedOutcome {
        self.fail_pending_semantic_receipt();
        self.fail_pending_action_result();
        drop(send_host_end(stream, &self.spec.backend_session_id).await);
        ConnectedOutcome::Stop
    }

    async fn handle_heartbeat(&mut self, stream: &mut HostWebSocketStream) -> ConnectedOutcome {
        if self
            .pending_action_result
            .as_ref()
            .is_some_and(|pending| time::Instant::now() >= pending.deadline)
        {
            self.fail_pending_action_result();
            return self
                .mark_reconnecting("host action-result acknowledgement timed out".to_owned())
                .await;
        }
        if self
            .pending_semantic_receipt
            .as_ref()
            .is_some_and(|pending| time::Instant::now() >= pending.deadline)
        {
            self.fail_pending_semantic_receipt();
            return self
                .mark_reconnecting("host semantic receipt acknowledgement timed out".to_owned())
                .await;
        }
        match send_heartbeat(stream, &self.spec.backend_session_id).await {
            Ok(()) => ConnectedOutcome::Continue,
            Err(error) => {
                self.mark_reconnecting(format!("host relay disconnected: {error}"))
                    .await
            }
        }
    }

    async fn handle_terminal_event(
        &mut self,
        event: Option<HostRelayTerminalEvent>,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        let Some(event) = event else {
            if cancellation_arrived_after_terminal_close(&self.cancellation).await {
                return self.handle_cancelled(stream).await;
            }
            return self
                .mark_terminal_closed("host relay terminal event channel closed")
                .await;
        };

        match event {
            HostRelayTerminalEvent::Raw { sequence, bytes } => {
                if !self.live_stream.required {
                    return ConnectedOutcome::Continue;
                }
                let Some(expected) = self.terminal.expected_raw_sequence else {
                    return ConnectedOutcome::Continue;
                };
                if sequence < expected {
                    return ConnectedOutcome::Continue;
                }
                if sequence > expected {
                    return self
                        .mark_terminal_closed(&format!(
                            "host relay raw sequence gap: expected {expected}, received {sequence}"
                        ))
                        .await;
                }
                let Some(next) = expected.checked_add(1) else {
                    return self
                        .mark_terminal_closed("host relay raw sequence space exhausted")
                        .await;
                };
                self.terminal.expected_raw_sequence = Some(next);
                self.terminal.pending_raw.push(bytes);
                self.flush_raw_batch(stream).await
            }
            HostRelayTerminalEvent::ForceCheckpoint => {
                self.terminal.checkpoint_pending = true;
                self.flush_checkpoint(stream).await
            }
            HostRelayTerminalEvent::ForcePresentation => {
                self.terminal.presentation_pending = true;
                ConnectedOutcome::Continue
            }
            HostRelayTerminalEvent::Closed { final_sequence } => {
                if self
                    .terminal
                    .expected_raw_sequence
                    .is_some_and(|expected| expected != final_sequence)
                {
                    return self
                        .mark_terminal_closed("terminal close boundary does not match raw cursor")
                        .await;
                }
                if !self.terminal.pending_raw.is_empty() {
                    let outcome = self.flush_raw_batch(stream).await;
                    if outcome != ConnectedOutcome::Continue {
                        return outcome;
                    }
                }
                self.handle_cancelled(stream).await
            }
        }
    }

    async fn flush_raw_batch(&mut self, stream: &mut HostWebSocketStream) -> ConnectedOutcome {
        if self.terminal.pending_raw.is_empty() {
            return ConnectedOutcome::Continue;
        }
        let Some(next_sequence) = self.terminal.expected_raw_sequence else {
            return self
                .mark_terminal_closed("host relay raw batch has no sequence cursor")
                .await;
        };
        let Ok(chunk_count) = u64::try_from(self.terminal.pending_raw.len()) else {
            return self
                .mark_terminal_closed("host relay raw batch chunk count overflowed")
                .await;
        };
        let Some(first_sequence) = next_sequence.checked_sub(chunk_count) else {
            return self
                .mark_terminal_closed("host relay raw batch sequence underflowed")
                .await;
        };
        let frame = match super::transport::build_raw_batch_frame(
            &mut self.encryption,
            first_sequence,
            &self.terminal.pending_raw,
        ) {
            Ok(frame) => frame,
            Err(error) => {
                if error.is_relay_reservation_exhausted() {
                    return self.retire_exhausted_reservation(&error).await;
                }
                return self
                    .mark_terminal_closed(&format!("host relay raw batch encode failed: {error}"))
                    .await;
            }
        };
        match super::wire::send_binary(stream, frame).await {
            Ok(()) => {
                self.terminal.pending_raw.clear();
                if self.live_stream.required {
                    self.terminal.presentation_pending = true;
                }
                ConnectedOutcome::Continue
            }
            Err(error) => {
                self.mark_reconnecting(format!("host relay raw batch send failed: {error}"))
                    .await
            }
        }
    }

    async fn flush_checkpoint(&mut self, stream: &mut HostWebSocketStream) -> ConnectedOutcome {
        if !self.terminal.checkpoint_pending || !self.live_stream.required {
            return ConnectedOutcome::Continue;
        }
        match send_checkpoint(
            stream,
            &self.spec,
            &mut self.terminal.checkpoint_revision,
            &mut self.encryption,
        )
        .await
        {
            Ok(next_sequence) => {
                self.terminal.checkpoint_pending = false;
                self.terminal.expected_raw_sequence = Some(next_sequence);
                ConnectedOutcome::Continue
            }
            Err(error) => {
                if error.is_relay_reservation_exhausted() {
                    self.retire_exhausted_reservation(&error).await
                } else {
                    self.mark_reconnecting(format!("host relay checkpoint stream failed: {error}"))
                        .await
                }
            }
        }
    }

    async fn handle_presentation_tick(
        &mut self,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        if !self.live_stream.required {
            self.terminal.presentation_pending = false;
            return ConnectedOutcome::Continue;
        }
        match send_presentation(
            stream,
            &self.spec,
            &mut self.terminal.presentation_revision,
            &mut self.encryption,
        )
        .await
        {
            Ok(()) => {
                self.terminal.presentation_pending = false;
                ConnectedOutcome::Continue
            }
            Err(error) => {
                if error.is_relay_reservation_exhausted() {
                    self.retire_exhausted_reservation(&error).await
                } else {
                    self.mark_reconnecting(format!(
                        "host relay presentation stream failed: {error}"
                    ))
                    .await
                }
            }
        }
    }

    async fn flush_pending_permissions(
        &mut self,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        let Some(snapshot) = self.pending_permissions_rx.borrow_and_update().clone() else {
            self.pending_permissions.send_required = false;
            return ConnectedOutcome::Continue;
        };
        if snapshot.incarnation_id != self.spec.backend_incarnation_id {
            return self
                .mark_terminal_closed(
                    "pending-permissions snapshot targeted another session incarnation",
                )
                .await;
        }
        let frame = match super::transport::build_pending_permissions_frame(
            &mut self.encryption,
            self.spec.id,
            &snapshot,
        ) {
            Ok(frame) => frame,
            Err(error) if error.is_relay_reservation_exhausted() => {
                return self.retire_exhausted_reservation(&error).await;
            }
            Err(error) => {
                return self
                    .mark_reconnecting(format!(
                        "host pending-permissions snapshot encryption failed: {error}"
                    ))
                    .await;
            }
        };
        match super::wire::send_binary(stream, frame).await {
            Ok(()) => {
                self.pending_permissions.send_required = false;
                ConnectedOutcome::Continue
            }
            Err(error) => {
                self.mark_reconnecting(format!(
                    "host pending-permissions snapshot send failed: {error}"
                ))
                .await
            }
        }
    }

    async fn handle_websocket_message(
        &mut self,
        message: Option<Result<Message, WebSocketError>>,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        match message {
            Some(Ok(Message::Text(text))) => self.handle_text_message(text.as_ref(), stream).await,
            Some(Ok(Message::Close(frame))) => self.handle_close_frame(frame.as_ref()).await,
            None => {
                self.mark_reconnecting("host relay closed by backend".to_owned())
                    .await
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => ConnectedOutcome::Continue,
            Some(Ok(Message::Binary(_) | Message::Frame(_))) => {
                self.mark_terminal_closed("unsupported non-text host relay frame")
                    .await
            }
            Some(Err(error)) => {
                self.mark_reconnecting(format!("host relay websocket failed: {error}"))
                    .await
            }
        }
    }

    async fn handle_text_message(
        &mut self,
        text: &str,
        stream: &mut HostWebSocketStream,
    ) -> ConnectedOutcome {
        let envelope: HostEnvelope = match serde_json::from_str(text) {
            Ok(envelope) => envelope,
            Err(error) => {
                return self
                    .mark_terminal_closed(&format!("malformed host relay envelope: {error}"))
                    .await;
            }
        };
        match super::HostIncomingMessageType::parse(&envelope.message_type) {
            Some(super::HostIncomingMessageType::SemanticReceiptAck) => {
                return self.handle_semantic_receipt_ack(text).await;
            }
            Some(super::HostIncomingMessageType::ActionResultAck) => {
                return self.handle_action_result_ack(text).await;
            }
            _ => {}
        }
        let was_required = self.live_stream.required;
        let outcome = handle_owner_message(
            &self.spec,
            stream,
            text,
            &mut self.live_stream,
            &mut self.terminal.presentation_pending,
            &mut self.fence_dedupe,
            self.event_sink.as_ref(),
        )
        .await;
        if !was_required && self.live_stream.required {
            self.terminal.checkpoint_pending = true;
            self.terminal.expected_raw_sequence = None;
            self.terminal.pending_raw.clear();
        } else if was_required && !self.live_stream.required {
            self.terminal.checkpoint_pending = false;
            self.terminal.expected_raw_sequence = None;
            self.terminal.pending_raw.clear();
        }
        match outcome {
            Ok(OwnerMessageOutcome::Continue) => ConnectedOutcome::Continue,
            Ok(OwnerMessageOutcome::ParticipantActionRejected { reason }) => {
                tracing::warn!(
                    session_id = %self.spec.id,
                    reason,
                    "participant action rejected; keeping host relay connected",
                );
                ConnectedOutcome::Continue
            }

            Ok(OwnerMessageOutcome::ProtocolDrift { reason }) => {
                self.mark_terminal_closed(&reason).await
            }
            Err(error) => {
                self.mark_reconnecting(format!("host relay receive failed: {error}"))
                    .await
            }
        }
    }

    async fn handle_action_result_ack(&mut self, text: &str) -> ConnectedOutcome {
        let ack: HostActionResultAckMessage = match serde_json::from_str(text) {
            Ok(ack) => ack,
            Err(error) => {
                return self
                    .mark_terminal_closed(&format!("malformed host.actionResultAck: {error}"))
                    .await;
            }
        };
        let Some(pending) = self.pending_action_result.as_ref() else {
            return self
                .mark_terminal_closed("unexpected action-result acknowledgement")
                .await;
        };
        if ack.session_id != self.spec.backend_session_id
            || ack.incarnation_id != pending.result.incarnation_id
            || ack.action_id != pending.result.action_id
            || ack.requester_user_id != pending.result.requester_user_id
        {
            return self
                .mark_terminal_closed("action-result acknowledgement target mismatch")
                .await;
        }
        if let Some(pending) = self.pending_action_result.take() {
            let _ignored = pending.delivery.send(true);
        }
        ConnectedOutcome::Continue
    }

    async fn handle_semantic_receipt_ack(&mut self, text: &str) -> ConnectedOutcome {
        let ack: HostSemanticReceiptAckMessage = match serde_json::from_str(text) {
            Ok(ack) => ack,
            Err(error) => {
                return self
                    .mark_terminal_closed(&format!("malformed host.semanticReceiptAck: {error}"))
                    .await;
            }
        };
        let Some(pending) = self.pending_semantic_receipt.as_ref() else {
            return self
                .mark_terminal_closed("unexpected semantic receipt acknowledgement")
                .await;
        };
        let matches =
            semantic_receipt_ack_matches(&self.spec.backend_session_id, &pending.receipt, &ack);
        if !matches {
            return self
                .mark_terminal_closed("semantic receipt acknowledgement target mismatch")
                .await;
        }
        if let Some(pending) = self.pending_semantic_receipt.take() {
            let _ignored = pending.delivery.send(true);
        }
        ConnectedOutcome::Continue
    }

    async fn handle_close_frame(
        &mut self,
        frame: Option<&tokio_tungstenite::tungstenite::protocol::CloseFrame>,
    ) -> ConnectedOutcome {
        match close_reasons::interpret(frame) {
            CloseBehavior::AuthRefresh => {
                self.force_refresh_token = self.last_access_token.clone();
                self.mark_reconnecting("host relay closed by backend".to_owned())
                    .await
            }
            CloseBehavior::Terminal { reason } => self.mark_terminal_closed(&reason).await,
            CloseBehavior::TerminalReplayGap => {
                self.mark_terminal_closed(close_reasons::TERMINAL_REPLAY_GAP_REASON)
                    .await
            }
            CloseBehavior::Retryable => {
                self.mark_reconnecting("host relay closed by backend".to_owned())
                    .await
            }
        }
    }
}

async fn cancellation_arrived_after_terminal_close(cancellation: &CancellationToken) -> bool {
    tokio::select! {
        biased;
        () = cancellation.cancelled() => true,
        () = time::sleep(TERMINAL_CLOSE_CANCELLATION_GRACE) => false,
    }
}

fn semantic_receipt_ack_matches(
    backend_session_id: &str,
    receipt: &super::HostRelaySemanticReceipt,
    ack: &HostSemanticReceiptAckMessage,
) -> bool {
    ack.session_id == backend_session_id
        && ack.incarnation_id == receipt.incarnation_id
        && ack.request_id == receipt.request_id
        && ack.requester_user_id == receipt.requester_user_id
        && ack.requester_device_id == receipt.requester_device_id
}

#[cfg(test)]
mod tests {
    use tokio::time;

    use super::{
        TERMINAL_CLOSE_CANCELLATION_GRACE, cancellation_arrived_after_terminal_close,
        semantic_receipt_ack_matches,
    };
    use crate::relay::{HostRelaySemanticReceipt, wire::HostSemanticReceiptAckMessage};
    use crate::session_relay::wire::{RelaySemanticMode, RelaySemanticOutcome};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn semantic_receipt_ack_requires_exact_target_tuple() {
        let incarnation_id = uuid::Uuid::now_v7();
        let request_id = uuid::Uuid::now_v7();
        let receipt = HostRelaySemanticReceipt {
            request_id,
            incarnation_id,
            mode: RelaySemanticMode::Steer,
            payload_sha256: "a".repeat(64),
            outcome: RelaySemanticOutcome::Injected,
            requester_user_id: "requester".to_owned(),
            requester_device_id: "requester-device".to_owned(),
            owner_user_id: "owner".to_owned(),
            owner_device_id: "owner-device".to_owned(),
            signature: None,
        };
        let exact = HostSemanticReceiptAckMessage {
            session_id: "session".to_owned(),
            incarnation_id,
            request_id,
            requester_user_id: "requester".to_owned(),
            requester_device_id: "requester-device".to_owned(),
        };
        assert!(semantic_receipt_ack_matches("session", &receipt, &exact));
        let wrong_device = HostSemanticReceiptAckMessage {
            requester_device_id: "other-device".to_owned(),
            ..exact
        };
        assert!(!semantic_receipt_ack_matches(
            "session",
            &receipt,
            &wrong_device
        ));
    }

    #[tokio::test]
    async fn terminal_close_only_counts_as_session_end_when_cancellation_arrives() {
        let ending = CancellationToken::new();
        let delayed_cancel = ending.clone();
        tokio::spawn(async move {
            time::sleep(TERMINAL_CLOSE_CANCELLATION_GRACE / 2).await;
            delayed_cancel.cancel();
        });
        assert!(cancellation_arrived_after_terminal_close(&ending).await);

        let detached = CancellationToken::new();
        assert!(!cancellation_arrived_after_terminal_close(&detached).await);
    }
}
