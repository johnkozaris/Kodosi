use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::{Runtime, identity::DeviceKeyAccess};
use crate::{AppError, Result};
use kodosi_backend_client::{
    api::{SemanticReceiptAckRequest, SemanticReceiptDto, SemanticReceiptPageDto},
    http_client::BackendHttpClient,
    session_relay::wire::{
        ParticipantSemanticReceiptMessage, RelaySemanticMode, RelaySemanticOutcome,
    },
};
use kodosi_domain::ids::SessionId;

const MAILBOX_BATCH_SIZE: usize = 32;

#[derive(Debug)]
pub(crate) struct SemanticMailboxCompletion {
    account_user_id: String,
    account_epoch: u64,
    device_id: String,
    uploaded: Vec<SemanticUploadIdentity>,
    acknowledged: Vec<uuid::Uuid>,
    page: Option<SemanticReceiptPageDto>,
    logs: Vec<String>,
}

#[derive(Debug, Clone)]
struct SemanticUpload {
    identity: SemanticUploadIdentity,
    envelope: SemanticReceiptDto,
}

#[derive(Debug, Clone)]
struct SemanticUploadIdentity {
    request: uuid::Uuid,
    session: SessionId,
    incarnation: uuid::Uuid,
}

#[derive(Debug, Clone)]
struct SemanticAcknowledgement {
    request_id: uuid::Uuid,
    request: SemanticReceiptAckRequest,
}

#[derive(Debug)]
struct SemanticMailboxWork {
    account_user_id: String,
    account_epoch: u64,
    device_id: String,
    signing_pkcs8: zeroize::Zeroizing<Vec<u8>>,
    cursor: Option<String>,
    uploads: Vec<SemanticUpload>,
    acknowledgements: Vec<SemanticAcknowledgement>,
}

fn receipt_transition(outcome: RelaySemanticOutcome) -> crate::host_protocol::SteerTransition {
    match outcome {
        RelaySemanticOutcome::Injected => crate::host_protocol::SteerTransition::Injected,
        RelaySemanticOutcome::Cancelled => crate::host_protocol::SteerTransition::Cancelled,
        RelaySemanticOutcome::DeliveryUnknown => {
            crate::host_protocol::SteerTransition::DeliveryUnknown
        }
    }
}

impl Runtime {
    pub(crate) async fn pump_semantic_receipt_mailbox(&mut self) {
        if self
            .semantic_mailbox_task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
            && let Some(task) = self.semantic_mailbox_task.take()
        {
            match task.await {
                Ok(completion) => self.apply_semantic_mailbox_completion(completion),
                Err(error) if error.is_cancelled() => {}
                Err(error) => self
                    .state
                    .record_log(format!("semantic mailbox worker failed: {error}")),
            }
        }
        if self.semantic_mailbox_task.is_some() || !self.remote_surfaces_ready() {
            return;
        }
        let Some(work) = self.prepare_semantic_mailbox_work() else {
            return;
        };
        let backend = self.backend.clone();
        self.semantic_mailbox_task = Some(tokio::spawn(execute_semantic_mailbox(backend, work)));
    }

    fn prepare_semantic_mailbox_work(&mut self) -> Option<SemanticMailboxWork> {
        let account_user_id = self.state.identity.auth.subject_string()?;
        let account_epoch = self.state.identity.account_epoch().value();
        let keys = match DeviceKeyAccess::new(&self.state.identity.auth, &self.device_key_store)
            .load_authenticated_device_keys()
        {
            Ok(keys) => keys,
            Err(error) => {
                self.state.record_log(format!(
                    "semantic receipt mailbox device unavailable: {error}"
                ));
                return None;
            }
        };
        let device_id = keys.device_id.clone();
        let signing_pkcs8 = keys.signing_pkcs8_bytes();
        let uploads = self.prepare_semantic_uploads(&account_user_id, &device_id, signing_pkcs8);
        let acknowledgements =
            self.prepare_semantic_acknowledgements(&account_user_id, &device_id, signing_pkcs8);
        Some(SemanticMailboxWork {
            account_user_id,
            account_epoch,
            device_id,
            signing_pkcs8: zeroize::Zeroizing::new(signing_pkcs8.to_vec()),
            cursor: self.semantic_receipt_cursor.clone(),
            uploads,
            acknowledgements,
        })
    }

    fn prepare_semantic_uploads(
        &mut self,
        account_user_id: &str,
        owner_device_id: &str,
        signing_pkcs8: &[u8],
    ) -> Vec<SemanticUpload> {
        let receipts = self
            .state
            .steering
            .pending_relay_receipts_for_account(account_user_id);
        receipts
            .into_iter()
            .take(MAILBOX_BATCH_SIZE)
            .filter_map(|receipt| {
                let request_id = uuid::Uuid::parse_str(&receipt.request_id).ok()?;
                let incarnation_id = uuid::Uuid::parse_str(&receipt.session_incarnation_id).ok()?;
                let session_id = SessionId::parse_field(&receipt.session_id, "sessionId").ok()?;
                let requester_device_id = self
                    .state
                    .steering
                    .relay_requester_device(&receipt.account_user_id, &receipt.request_id)?
                    .to_owned();
                let mode = super::steering::relay_mode(receipt.mode);
                let outcome = super::steering::relay_outcome(receipt.outcome)?;
                let wire = kodosi_backend_client::relay::HostRelaySemanticReceipt {
                    request_id,
                    incarnation_id,
                    mode,
                    payload_sha256: receipt.payload_sha256,
                    outcome,
                    requester_user_id: receipt.account_user_id.clone(),
                    requester_device_id,
                    owner_user_id: receipt.account_user_id,
                    owner_device_id: owner_device_id.to_owned(),
                    signature: None,
                };
                match kodosi_backend_client::relay::build_semantic_receipt_dto(
                    &receipt.session_id,
                    signing_pkcs8,
                    &wire,
                ) {
                    Ok(envelope) => Some(SemanticUpload {
                        identity: SemanticUploadIdentity {
                            request: request_id,
                            session: session_id,
                            incarnation: incarnation_id,
                        },
                        envelope,
                    }),
                    Err(error) => {
                        self.state.record_log(format!(
                            "{} semantic receipt envelope failed: {error}",
                            session_id.short()
                        ));
                        None
                    }
                }
            })
            .collect()
    }

    fn prepare_semantic_acknowledgements(
        &mut self,
        account_user_id: &str,
        requester_device_id: &str,
        signing_pkcs8: &[u8],
    ) -> Vec<SemanticAcknowledgement> {
        self.remote_semantics
            .unacknowledged_completed(account_user_id)
            .into_iter()
            .take(MAILBOX_BATCH_SIZE)
            .filter_map(|receipt| {
                match semantic_acknowledgement(
                    account_user_id,
                    requester_device_id,
                    signing_pkcs8,
                    &receipt.request,
                ) {
                    Ok(request) => Some(SemanticAcknowledgement {
                        request_id: receipt.request.request_id,
                        request,
                    }),
                    Err(error) => {
                        self.state.record_log(format!(
                            "semantic mailbox ack preparation failed: {error}"
                        ));
                        None
                    }
                }
            })
            .collect()
    }

    fn apply_semantic_mailbox_completion(&mut self, completion: SemanticMailboxCompletion) {
        let context_matches = self.state.identity.auth.subject_string().as_deref()
            == Some(completion.account_user_id.as_str())
            && self.state.identity.account_epoch().value() == completion.account_epoch
            && DeviceKeyAccess::new(&self.state.identity.auth, &self.device_key_store)
                .load_authenticated_device_keys()
                .is_ok_and(|keys| keys.device_id == completion.device_id);
        if !context_matches {
            self.semantic_receipt_cursor = None;
            return;
        }
        for message in completion.logs {
            self.state.record_log(message);
        }
        for uploaded in completion.uploaded {
            if let Err(error) = self.state.steering.acknowledge_relay_receipt(
                &completion.account_user_id,
                uploaded.request,
                uploaded.session,
                uploaded.incarnation,
            ) {
                self.state.record_log(format!(
                    "{} semantic mailbox upload committed but local receipt retirement failed: {error}",
                    uploaded.session.short()
                ));
                break;
            }
        }
        for request_id in completion.acknowledged {
            match self
                .remote_semantics
                .acknowledge_backend(&completion.account_user_id, request_id)
            {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    self.state.record_log(format!(
                        "semantic mailbox ack committed but local retirement failed: {error}"
                    ));
                    break;
                }
            }
        }
        if let Some(page) = completion.page {
            self.apply_semantic_receipt_page(&completion.account_user_id, page);
        }
    }

    fn apply_semantic_receipt_page(&mut self, account_user_id: &str, page: SemanticReceiptPageDto) {
        for envelope in page.items {
            let receipt = match participant_receipt(&envelope) {
                Ok(receipt) => receipt,
                Err(error) => {
                    self.state
                        .record_log(format!("semantic mailbox receipt rejected: {error}"));
                    continue;
                }
            };
            match self.remote_semantics.complete(account_user_id, &receipt) {
                Ok(Some(completed)) => self.publish_remote_semantic_completion(&completed),
                Ok(None) => {}
                Err(error) => self.state.record_log(format!(
                    "{} semantic mailbox receipt verification failed: {error}",
                    envelope.session_id
                )),
            }
        }
        self.semantic_receipt_cursor = page.next_cursor;
    }

    fn publish_remote_semantic_completion(
        &mut self,
        completed: &super::remote_semantics::RemoteSemanticReceipt,
    ) {
        let entry = super::remote_semantics::to_entry_public(completed);
        self.state
            .runtime_outbox
            .queue_agent_intel(crate::AgentIntelEvent::SteerState {
                entry,
                transition: receipt_transition(completed.outcome),
                message: None,
            });
    }
}

async fn execute_semantic_mailbox(
    backend: BackendHttpClient,
    work: SemanticMailboxWork,
) -> SemanticMailboxCompletion {
    let account_user_id = work.account_user_id.clone();
    let device_id = work.device_id.clone();
    let completion_device_id = device_id.clone();
    let mut completion = SemanticMailboxCompletion {
        account_user_id: account_user_id.clone(),
        account_epoch: work.account_epoch,
        device_id: completion_device_id,
        uploaded: Vec::new(),
        acknowledged: Vec::new(),
        page: None,
        logs: Vec::new(),
    };
    for upload in &work.uploads {
        let proof = kodosi_backend_client::api::DeviceHttpProof {
            user_id: &account_user_id,
            device_id: &device_id,
            signing_pkcs8: work.signing_pkcs8.as_ref(),
        };
        match backend
            .upload_semantic_receipt(proof, &upload.envelope)
            .await
        {
            Ok(()) => completion.uploaded.push(upload.identity.clone()),
            Err(error) => {
                completion
                    .logs
                    .push(format!("semantic mailbox upload deferred: {error}"));
                break;
            }
        }
    }
    for acknowledgement in &work.acknowledgements {
        let proof = kodosi_backend_client::api::DeviceHttpProof {
            user_id: &account_user_id,
            device_id: &device_id,
            signing_pkcs8: work.signing_pkcs8.as_ref(),
        };
        match backend
            .acknowledge_semantic_receipt(proof, &acknowledgement.request)
            .await
        {
            Ok(()) => completion.acknowledged.push(acknowledgement.request_id),
            Err(error) => {
                completion
                    .logs
                    .push(format!("semantic mailbox ack deferred: {error}"));
                break;
            }
        }
    }
    let proof = kodosi_backend_client::api::DeviceHttpProof {
        user_id: &account_user_id,
        device_id: &device_id,
        signing_pkcs8: work.signing_pkcs8.as_ref(),
    };
    match backend
        .fetch_semantic_receipts(proof, MAILBOX_BATCH_SIZE, work.cursor.as_deref())
        .await
    {
        Ok(page) => completion.page = Some(page),
        Err(error) => completion
            .logs
            .push(format!("semantic mailbox fetch deferred: {error}")),
    }
    completion
}

fn semantic_acknowledgement(
    account_user_id: &str,
    requester_device_id: &str,
    signing_pkcs8: &[u8],
    request: &super::remote_semantics::RemoteSemanticRequest,
) -> Result<SemanticReceiptAckRequest> {
    let requester_user_id =
        uuid::Uuid::parse_str(account_user_id).map_err(|_| AppError::InvalidBackendData {
            field: "semanticReceipt.requesterUserId".to_owned(),
            reason: "account identity is not a UUID".to_owned(),
        })?;
    let session_id =
        uuid::Uuid::parse_str(&request.session_id).map_err(|_| AppError::InvalidBackendData {
            field: "semanticReceipt.sessionId".to_owned(),
            reason: "session identity is not a UUID".to_owned(),
        })?;
    let preimage = kodosi_backend_client::crypto::semantic_receipt_ack_preimage(
        &session_id,
        &request.incarnation_id,
        &request.request_id,
        &requester_user_id,
        requester_device_id,
    )?;
    let signature = kodosi_backend_client::crypto::sign_control_message(signing_pkcs8, &preimage)?;
    Ok(SemanticReceiptAckRequest {
        session_id,
        incarnation_id: request.incarnation_id,
        request_id: request.request_id,
        requester_user_id: requester_user_id.to_string(),
        requester_device_id: requester_device_id.to_owned(),
        signature: BASE64.encode(signature),
    })
}

fn participant_receipt(envelope: &SemanticReceiptDto) -> Result<ParticipantSemanticReceiptMessage> {
    Ok(ParticipantSemanticReceiptMessage {
        session_id: envelope.session_id.to_string(),
        incarnation_id: envelope.incarnation_id,
        request_id: envelope.request_id,
        mode: parse_mode(&envelope.mode)?,
        payload_sha256: envelope.payload_sha256.clone(),
        outcome: parse_outcome(&envelope.outcome)?,
        requester_user_id: envelope.requester_user_id.to_string(),
        requester_device_id: envelope.requester_device_id.clone(),
        owner_user_id: envelope.owner_user_id.to_string(),
        owner_device_id: envelope.owner_device_id.clone(),
        signature: envelope.signature.clone(),
    })
}

fn parse_mode(value: &str) -> Result<RelaySemanticMode> {
    match value {
        "queue" => Ok(RelaySemanticMode::Queue),
        "steer" => Ok(RelaySemanticMode::Steer),
        "stopAndSend" => Ok(RelaySemanticMode::StopAndSend),
        _ => Err(AppError::Unsupported {
            reason: "semantic mailbox mode is invalid".to_owned(),
        }),
    }
}

fn parse_outcome(value: &str) -> Result<RelaySemanticOutcome> {
    match value {
        "injected" => Ok(RelaySemanticOutcome::Injected),
        "cancelled" => Ok(RelaySemanticOutcome::Cancelled),
        "deliveryUnknown" => Ok(RelaySemanticOutcome::DeliveryUnknown),
        _ => Err(AppError::Unsupported {
            reason: "semantic mailbox outcome is invalid".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{SemanticMailboxWork, execute_semantic_mailbox, receipt_transition};
    use crate::{config::AppConfig, runtime::Runtime};
    use kodosi_backend_client::session_relay::wire::RelaySemanticOutcome;
    use tokio::io::AsyncReadExt;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn delivery_unknown_receipt_publishes_delivery_unknown_transition() {
        assert_eq!(
            receipt_transition(RelaySemanticOutcome::DeliveryUnknown),
            crate::host_protocol::SteerTransition::DeliveryUnknown
        );
    }

    #[tokio::test]
    async fn hanging_mailbox_http_does_not_block_local_commands() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("request");
            assert!(read > 0);
            accepted_tx.send(()).ok();
            std::future::pending::<()>().await;
        });

        let mut config = AppConfig::default();
        config.backend.api = Some(format!("http://{address}/"));
        config.auth.keyring_service = format!("kodosi.mailbox.test.{}", uuid::Uuid::now_v7());
        let mut app = Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .expect("runtime");
        let work = SemanticMailboxWork {
            account_user_id: "account".to_owned(),
            account_epoch: 1,
            device_id: "device".to_owned(),
            signing_pkcs8: zeroize::Zeroizing::new(Vec::new()),
            cursor: None,
            uploads: Vec::new(),
            acknowledgements: Vec::new(),
        };
        app.semantic_mailbox_task = Some(tokio::spawn(execute_semantic_mailbox(
            app.backend.clone(),
            work,
        )));
        tokio::time::timeout(std::time::Duration::from_secs(1), accepted_rx)
            .await
            .expect("worker should dispatch GET")
            .expect("server should observe GET");

        let effects = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            crate::host_protocol::sessions_command::apply_session_command(
                &mut app,
                crate::host_protocol::SessionCommand::SnapshotRefresh,
            ),
        )
        .await
        .expect("hanging mailbox HTTP must not block local commands")
        .expect("snapshot command should succeed");
        assert!(effects.defer_catalog_replay);
        server.abort();
    }
}
