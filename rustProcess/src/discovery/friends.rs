use kodosi_backend_client::http_client::BackendHttpClient;

use super::friend_adapters;
use crate::{
    AppError, Result, host_protocol::FriendsEvent,
    runtime::runtime_event_outbox::RuntimeEventOutbox,
};

pub(crate) struct FriendsCtx<'a> {
    pub(crate) backend: &'a BackendHttpClient,
    pub(crate) outbox: &'a mut RuntimeEventOutbox,
}

impl FriendsCtx<'_> {
    pub(crate) async fn refresh_snapshot(&mut self, operation: &str) {
        self.refresh_snapshot_for(operation, None).await;
    }

    async fn refresh_snapshot_for(&mut self, operation: &str, request_id: Option<String>) {
        match self.fetch_snapshot(request_id.clone()).await {
            Ok(event) => self.outbox.queue_friends(event),
            Err(err) => self.emit_error(operation, &err, request_id),
        }
    }

    pub(crate) async fn send_request(&mut self, username: &str, request_id: String) {
        if let Err(err) = self.backend.send_friend_request(username).await {
            self.emit_error("request.send", &err, Some(request_id));
            return;
        }
        self.refresh_snapshot_for("request.send", Some(request_id))
            .await;
    }

    pub(crate) async fn accept_request(&mut self, username: &str) {
        if let Err(err) = self.backend.accept_friend_request(username).await {
            self.emit_error("request.accept", &err, None);
            return;
        }
        self.refresh_snapshot("request.accept").await;
    }

    pub(crate) async fn reject_request(&mut self, username: &str) {
        if let Err(err) = self.backend.reject_friend_request(username).await {
            self.emit_error("request.reject", &err, None);
            return;
        }
        self.refresh_snapshot("request.reject").await;
    }

    pub(crate) async fn cancel_request(&mut self, username: &str) {
        if let Err(err) = self.backend.cancel_outgoing_friend_request(username).await {
            self.emit_error("request.cancel", &err, None);
            return;
        }
        self.refresh_snapshot("request.cancel").await;
    }

    pub(crate) async fn remove_friend(&mut self, handle: &str) {
        if let Err(err) = self.backend.unfriend(handle).await {
            self.emit_error("remove", &err, None);
            return;
        }
        self.refresh_snapshot("remove").await;
    }

    async fn fetch_snapshot(&self, request_id: Option<String>) -> Result<FriendsEvent> {
        if !self.backend.is_configured() {
            return Err(AppError::Unsupported {
                reason: "backend is not configured".to_owned(),
            });
        }
        let (friends, inbox) = tokio::try_join!(
            self.backend.fetch_friends(),
            self.backend.fetch_friend_requests(),
        )?;
        Ok(friend_adapters::friends_snapshot(
            friends, inbox, request_id,
        ))
    }

    fn emit_error(
        &mut self,
        operation: &str,
        err: &impl std::fmt::Display,
        request_id: Option<String>,
    ) {
        tracing::debug!(%err, operation, "friends operation failed");
        self.outbox.queue_friends(FriendsEvent::Error {
            operation: operation.to_owned(),
            message: err.to_string(),
            request_id,
        });
    }
}
