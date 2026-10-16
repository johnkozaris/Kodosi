use super::{
    Arc, Bytes, CString, Gate, KODOSI_FFI_BUSY, KODOSI_FFI_DESER_FAILED, KODOSI_FFI_NULL_HANDLE,
    KODOSI_FFI_OK, KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_SESSION_NOT_FOUND,
    KODOSI_FFI_STALE_SUBSCRIPTION, KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED, MAX_SUBSCRIPTIONS,
    Mutex, RuntimeHandle, State, SubscriptionState, Uuid, c_char, c_void, caught, code, instance,
    lock, payload, terminal, text,
};

const REFRESH_RETRY: std::time::Duration = std::time::Duration::from_secs(1);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_input(
    handle: *mut c_void,
    session_id: *const c_char,
    expected_runtime_incarnation_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
    bytes: *const u8,
    len: usize,
) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let Some(session) = (unsafe { text(session_id) }).and_then(|s| Uuid::parse_str(s).ok())
        else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Some(incarnation) = (unsafe { text(expected_runtime_incarnation_id) })
            .and_then(|s| Uuid::parse_str(s).ok())
        else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Some(id) = (unsafe { text(subscription_id) }) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let bytes = match unsafe { payload(bytes, len) } {
            Ok(value) => value,
            Err(error) => return error,
        };
        let registry = lock(&instance.state.subscriptions);
        let Some(subscription) = registry
            .entries
            .get(&session)
            .filter(|s| s.matches(id, subscription_generation) && !s.cancel.is_cancelled())
        else {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        };
        let Some((connection, current)) = *lock(&subscription.connected) else {
            return KODOSI_FFI_SESSION_NOT_FOUND;
        };
        if incarnation != current {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        }
        let result = instance
            .runtime
            .input(
                session,
                incarnation,
                connection,
                Bytes::copy_from_slice(bytes),
            )
            .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK);
        drop(registry);
        result
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_connect(
    handle: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    generation: u64,
) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let Some(session_text) = (unsafe { text(session_id) }) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Ok(session) = Uuid::parse_str(session_text) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Some(id_text) = (unsafe { text(subscription_id) }).filter(|s| !s.is_empty()) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        if generation == 0 {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        }
        let (Ok(session_string), Ok(id)) = (CString::new(session_text), CString::new(id_text))
        else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let subscription = Arc::new(SubscriptionState {
            session: session_string,
            id,
            generation,
            cancel: instance.state.cancel.child_token(),
            refresh: tokio::sync::Notify::new(),
            gate: Gate::default(),
            connected: Mutex::new(None),
        });
        let prior = {
            let mut registry = lock(&instance.state.subscriptions);
            if instance.state.cancel.is_cancelled() {
                return KODOSI_FFI_RUNTIME_STOPPED;
            }
            if !registry.sessions.contains(&session) {
                return KODOSI_FFI_SESSION_NOT_FOUND;
            }
            if registry
                .generations
                .get(&session)
                .is_some_and(|last| generation <= *last)
            {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            }
            if !registry.generations.contains_key(&session)
                && registry.generations.len() >= MAX_SUBSCRIPTIONS
            {
                return KODOSI_FFI_BUSY;
            }
            registry.generations.insert(session, generation);
            registry.entries.insert(session, Arc::clone(&subscription))
        };
        if let Some(prior) = prior {
            prior.retire();
        }
        let state = Arc::clone(&instance.state);
        let runtime = instance.runtime.clone();
        let task_subscription = Arc::clone(&subscription);
        if !instance.spawn(async move {
            run_terminal(runtime, state, session, task_subscription).await;
        }) {
            subscription.retire();
            instance.state.remove(session, &subscription);
            return KODOSI_FFI_RUNTIME_STOPPED;
        }
        KODOSI_FFI_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_refresh(
    handle: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    generation: u64,
) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let Some(session) = (unsafe { text(session_id) }).and_then(|s| Uuid::parse_str(s).ok())
        else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Some(id) = (unsafe { text(subscription_id) }) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let registry = lock(&instance.state.subscriptions);
        let Some(entry) = registry
            .entries
            .get(&session)
            .filter(|s| s.matches(id, generation) && !s.cancel.is_cancelled())
        else {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        };
        entry.refresh.notify_one();
        drop(registry);
        KODOSI_FFI_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_disconnect(
    handle: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    generation: u64,
) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let Some(session) = (unsafe { text(session_id) }).and_then(|s| Uuid::parse_str(s).ok())
        else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Some(id) = (unsafe { text(subscription_id) }) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let entry = {
            let mut registry = lock(&instance.state.subscriptions);
            if !registry
                .entries
                .get(&session)
                .is_some_and(|s| s.matches(id, generation))
            {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            }
            registry.entries.remove(&session)
        };
        if let Some(entry) = entry {
            entry.retire();
            instance.state.remove(session, &entry);
        }
        KODOSI_FFI_OK
    })
}

async fn run_terminal(
    runtime: RuntimeHandle,
    state: Arc<State>,
    session: Uuid,
    token: Arc<SubscriptionState>,
) {
    connect_terminal(&runtime, &state, session, &token).await;
    token.retire();
    let connected = lock(&token.connected).take();
    if let Some((connection, _)) = connected {
        runtime.unsubscribe_terminal(session, connection).await;
    }
    state.remove(session, &token);
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
            token.retire();
            return;
        }
    };
    if !state.checkpoint(token, &subscriber.checkpoint, subscriber.next_sequence) {
        state.result(token, KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED);
        runtime
            .unsubscribe_terminal(session, subscriber.connection_id)
            .await;
        token.retire();
        return;
    }
    *lock(&token.connected) = Some((subscriber.connection_id, subscriber.incarnation_id));
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
                        state.data(token, &frame);
                        continue;
                    }
                    Ok(Some(terminal::Frame::Control(terminal::ControlFrame::Resize { rows, cols, at_sequence }))) => {
                        state.control(token, &serde_json::json!({"type":"term.resize","rows":rows,"cols":cols,"atSequence":at_sequence}));
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
