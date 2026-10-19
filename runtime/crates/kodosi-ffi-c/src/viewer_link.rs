use kodosi_runtime::{Event, RuntimeHandle};
use serde_json::Value;
use tokio::sync::broadcast;

use super::{
    Arc, Bytes, CommandEnvelope, Config, Error, Instance, KODOSI_FFI_OK,
    KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED, KODOSI_START_FAILED,
    KODOSI_START_REJECTED, KodosiStartFailure, State, SubscriptionState, Uuid, code, lock,
    start_failure, terminal,
};

const REFRESH_RETRY: std::time::Duration = std::time::Duration::from_secs(1);

pub(super) struct Link {
    runtime: RuntimeHandle,
}

pub(super) struct Runner {
    initial: Vec<Event>,
    events: broadcast::Receiver<Event>,
}

pub(super) struct Connected {
    pub(super) incarnation: Uuid,
    connection: Uuid,
}

impl Link {
    pub(super) async fn open(
        config: &Config,
    ) -> std::result::Result<(Self, Runner), Box<KodosiStartFailure>> {
        let failure = |error: &Error| {
            tracing::error!(%error, "could not start the viewer runtime");
            let code = match error {
                Error::Invalid(_) => KODOSI_START_REJECTED,
                _ => KODOSI_START_FAILED,
            };
            start_failure(code, &error.to_string())
        };
        let link = Self {
            runtime: kodosi_runtime::start(config.clone())
                .await
                .map_err(|error| failure(&error))?,
        };
        let (initial, events) = link
            .runtime
            .observe()
            .await
            .map_err(|error| failure(&error))?;
        Ok((link, Runner { initial, events }))
    }

    pub(super) fn command(&self, _value: Value, envelope: &CommandEnvelope) -> i32 {
        self.runtime
            .try_send(envelope.clone())
            .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK)
    }

    pub(super) fn connect_terminal(
        &self,
        instance: &Instance,
        session: Uuid,
        subscription: Arc<SubscriptionState>,
    ) -> bool {
        let runtime = self.runtime.clone();
        let state = Arc::clone(&instance.state);
        instance.spawn(async move {
            connect_terminal(&runtime, &state, session, &subscription).await;
            subscription.retire();
            let connected = lock(&subscription.connected).take();
            if let Some(connected) = connected {
                runtime
                    .unsubscribe_terminal(session, connected.connection)
                    .await;
            }
            state.remove(session, &subscription);
        })
    }
}

pub(super) async fn stopped(link: &Link) {
    link.runtime.stopped().await;
}

pub(super) async fn shutdown(link: &Link) {
    link.runtime.shutdown().await;
}

impl Connected {
    pub(super) fn control(
        &self,
        instance: &Instance,
        subscription: &Arc<SubscriptionState>,
        envelope: &CommandEnvelope,
    ) -> i32 {
        let runtime = instance.link.runtime.clone();
        let state = Arc::clone(&instance.state);
        let subscription = Arc::clone(subscription);
        let connection = self.connection;
        let envelope = envelope.clone();
        let spawned = instance.spawn(async move {
            match runtime.terminal_control(envelope, connection).await {
                Ok(Some(answer)) => state.control(&subscription, &answer),
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "terminal control was not applied"),
            }
        });
        if spawned {
            KODOSI_FFI_OK
        } else {
            KODOSI_FFI_RUNTIME_STOPPED
        }
    }

    pub(super) fn input(&self, instance: &Instance, session: Uuid, bytes: &[u8]) -> i32 {
        instance
            .link
            .runtime
            .input(
                session,
                self.incarnation,
                self.connection,
                Bytes::copy_from_slice(bytes),
            )
            .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK)
    }
}

impl Runner {
    pub(super) fn spawn(self, instance: &Instance) -> bool {
        let runtime = instance.link.runtime.clone();
        let state = Arc::clone(&instance.state);
        let Self {
            initial,
            mut events,
        } = self;
        instance.spawn(async move {
            for event in &initial {
                deliver(&state, event);
            }
            loop {
                tokio::select! {
                    biased;
                    () = state.cancel.cancelled() => break,
                    event = events.recv() => match event {
                        Ok(event) if event.reaches(runtime.consumer()) => deliver(&state, &event),
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            match runtime.observe().await {
                                Ok((snapshot, receiver)) => {
                                    events = receiver;
                                    for event in &snapshot { deliver(&state, event); }
                                }
                                Err(_) => break,
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        })
    }
}

fn deliver(state: &State, event: &Event) {
    if let Ok(value) = serde_json::to_value(event) {
        state.event(&value);
    }
}

async fn connect_terminal(
    runtime: &RuntimeHandle,
    state: &State,
    session: Uuid,
    token: &SubscriptionState,
) {
    let initial = tokio::select! {
        biased;
        () = token.cancel.cancelled() => return,
        result = runtime.subscribe_terminal(session) => result,
    };
    let subscriber = match initial {
        Ok(value) => value,
        Err(error) => {
            state.result(token, code(&error));
            return;
        }
    };
    if !state.checkpoint(token, &subscriber.checkpoint, subscriber.next_sequence) {
        state.result(token, KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED);
        runtime
            .unsubscribe_terminal(session, subscriber.connection_id)
            .await;
        return;
    }
    *lock(&token.connected) = Some(Connected {
        incarnation: subscriber.incarnation_id,
        connection: subscriber.connection_id,
    });
    state.result(token, KODOSI_FFI_OK);
    stream_terminal(runtime, state, session, token, subscriber).await;
}

async fn stream_terminal(
    runtime: &RuntimeHandle,
    state: &State,
    session: Uuid,
    token: &SubscriptionState,
    mut subscriber: terminal::Subscription,
) {
    let mut retry: Option<tokio::time::Instant> = None;
    loop {
        tokio::select! {
            biased;
            () = token.cancel.cancelled() => break,
            () = token.refresh.notified(), if subscriber.can_refresh() => {
                let fresh = tokio::select! {
                    biased;
                    () = token.cancel.cancelled() => break,
                    result = runtime.terminal_checkpoint(session, subscriber.connection_id) => result,
                };
                match fresh {
                    Ok(cut) => subscriber.refresh(cut),
                    Err(error) => {
                        tracing::warn!(%session, %error, "terminal view refresh failed; it will be tried again");
                        retry = Some(tokio::time::Instant::now() + REFRESH_RETRY);
                    }
                }
            }
            () = async { tokio::time::sleep_until(retry.unwrap_or_else(tokio::time::Instant::now)).await }, if retry.is_some() => {
                retry = None;
                token.refresh.notify_one();
            }
            frame = subscriber.recv() => {
                let (reason, final_sequence) = match frame {
                    Ok(Some(terminal::Frame::Data(frame))) => {
                        state.data(token, frame.sequence, &frame.bytes);
                        continue;
                    }
                    Ok(Some(terminal::Frame::Control(terminal::ControlFrame::Resize { rows, cols, cell, at_sequence }))) => {
                        state.control(token, &serde_json::json!({"type":"term.resize","rows":rows,"cols":cols,
                            "cellWidthPixels":cell.width,"cellHeightPixels":cell.height,"atSequence":at_sequence}));
                        continue;
                    }
                    Ok(Some(terminal::Frame::Control(terminal::ControlFrame::Snapshot { checkpoint, next_sequence }))) => {
                        if state.checkpoint(token, &checkpoint, next_sequence) {
                            retry = None;
                            continue;
                        }
                        ("the terminal snapshot could not be restored".to_owned(), subscriber.next_sequence)
                    }
                    Ok(Some(terminal::Frame::Control(terminal::ControlFrame::Closed { reason, final_sequence }))) => {
                        (reason, final_sequence)
                    }
                    Ok(None) => ("terminal disconnected".to_owned(), subscriber.next_sequence),
                    Err(error) => (error.to_string(), subscriber.next_sequence),
                };
                state.control(
                    token,
                    &serde_json::json!({"type":"term.closed","reason":reason,"finalSequence":final_sequence}),
                );
                break;
            }
        }
    }
}
