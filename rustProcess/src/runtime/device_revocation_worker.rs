#[cfg(test)]
mod tests;

#[cfg(feature = "cli")]
use tokio::sync::oneshot;

use super::{
    Runtime,
    identity::device_list::{DeviceListCtx, DeviceRevocationOutcome},
    runtime_event_outbox::RuntimeEventOutbox,
};
use crate::{AppError, Result, host_protocol::DeviceEvent};

pub(crate) enum Reply {
    Desktop,
    #[cfg(feature = "cli")]
    Rpc(oneshot::Sender<Result<super::identity::DeviceRpcOutcome>>),
}

pub(crate) struct Completion {
    outcome: Result<DeviceRevocationOutcome>,
    events: Vec<DeviceEvent>,
}

struct Pending {
    user_id: String,
    epoch: u64,
    reply: Reply,
    task: tokio::task::JoinHandle<Completion>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub(crate) struct DeviceRevocationWorker {
    pending: Option<Pending>,
}

impl DeviceRevocationWorker {
    pub(crate) fn start(&mut self, app: &Runtime, device_id: String, reply: Reply) -> Result<()> {
        let validation = crate::DeviceCommand::Revoke {
            device_id: device_id.clone(),
        }
        .validate()
        .map_err(|error| AppError::Unsupported {
            reason: error.to_string(),
        });
        let admission = if let Err(error) = validation {
            Err(error)
        } else if self.pending.is_some() {
            Err(AppError::Unsupported {
                reason: "Another device revocation is still pending.".to_owned(),
            })
        } else if !app.remote_surfaces_ready() {
            Err(AppError::Unsupported {
                reason: "Wait for account and device verification before revoking a device."
                    .to_owned(),
            })
        } else {
            app.state
                .identity
                .auth
                .subject_string()
                .ok_or(AppError::Unauthorized)
        };
        let user_id = match admission {
            Ok(user_id) => user_id,
            Err(error) => {
                #[cfg(feature = "cli")]
                if let Reply::Rpc(reply) = reply {
                    drop(reply.send(Err(error)));
                    return Ok(());
                }
                return Err(error);
            }
        };
        let (_, epoch) = app.state.identity.event_context();
        let auth = app.state.identity.auth.clone();
        let backend = app.backend.clone();
        let keys = app.device_key_store.clone();
        let pins = app.pin_store.clone();
        let cancelled = app.state.identity.account_cancellation();
        let history_path = app.room_roster_pins_path.clone();
        let shutdown = app.shutdown.clone();
        let task = tokio::spawn(async move {
            let mut outbox = RuntimeEventOutbox::default();
            let mut context = DeviceListCtx {
                auth: &auth,
                backend: &backend,
                device_key_store: &keys,
                pin_store: &pins,
                room_roster_pins_path: &history_path,
                outbox: &mut outbox,
            };
            let outcome = tokio::select! {
                biased;
                () = cancelled.cancelled() => Err(AppError::Unauthorized),
                () = shutdown.cancelled() => Err(AppError::Unauthorized),
                outcome = context.revoke(&device_id) => outcome,
            };
            Completion {
                outcome,
                events: outbox.drain_devices(),
            }
        });
        self.pending = Some(Pending {
            user_id,
            epoch,
            reply,
            task,
        });
        Ok(())
    }

    pub(crate) fn reconcile_account(&mut self, app: &Runtime) {
        if self.pending.as_ref().is_some_and(|pending| {
            let (user_id, epoch) = app.state.identity.event_context();
            user_id.as_deref() != Some(pending.user_id.as_str()) || epoch != pending.epoch
        }) {
            self.pending = None;
        }
    }

    pub(crate) async fn poll(&mut self) -> std::result::Result<Completion, tokio::task::JoinError> {
        match self.pending.as_mut() {
            Some(pending) => (&mut pending.task).await,
            None => std::future::pending().await,
        }
    }

    pub(crate) fn finish(
        &mut self,
        app: &mut Runtime,
        result: std::result::Result<Completion, tokio::task::JoinError>,
    ) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        let (user_id, epoch) = app.state.identity.event_context();
        if user_id.as_deref() != Some(pending.user_id.as_str()) || epoch != pending.epoch {
            return;
        }
        let completion = result.unwrap_or_else(|error| Completion {
            outcome: Err(AppError::Join(error)),
            events: Vec::new(),
        });
        if completion.outcome.is_ok() {
            app.invalidate_hosted_session_keys();
        } else if let Err(error) = &completion.outcome {
            super::auth::mark_expired_if_required(app, error);
        }
        for event in completion.events {
            app.state.runtime_outbox.queue_devices(event);
        }
        match std::mem::replace(&mut pending.reply, Reply::Desktop) {
            Reply::Desktop => match completion.outcome {
                Ok(outcome) => {
                    if let Some(message) = outcome.history_warning {
                        app.state.runtime_outbox.queue_devices(DeviceEvent::Error {
                            user_code: None,
                            operation: "revoke.history".to_owned(),
                            message,
                        });
                    }
                }
                Err(error) => app.state.runtime_outbox.queue_devices(DeviceEvent::Error {
                    user_code: None,
                    operation: "revoke".to_owned(),
                    message: error.to_string(),
                }),
            },
            #[cfg(feature = "cli")]
            Reply::Rpc(reply) => {
                drop(
                    reply.send(
                        completion
                            .outcome
                            .map(super::identity::DeviceRpcOutcome::Revoked),
                    ),
                );
            }
        }
    }
}
