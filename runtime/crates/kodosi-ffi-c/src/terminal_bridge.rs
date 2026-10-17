use super::{
    Arc, CString, Gate, KODOSI_FFI_BUSY, KODOSI_FFI_DESER_FAILED, KODOSI_FFI_NULL_HANDLE,
    KODOSI_FFI_OK, KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_SESSION_NOT_FOUND,
    KODOSI_FFI_STALE_SUBSCRIPTION, MAX_SUBSCRIPTIONS, Mutex, SubscriptionState, Uuid, c_char,
    c_void, caught, instance, lock, payload, text,
};

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
        let Some(subscription) = instance
            .state
            .subscription(session, id, subscription_generation)
        else {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        };
        let guard = lock(&subscription.connected);
        let Some(connected) = guard.as_ref() else {
            return KODOSI_FFI_SESSION_NOT_FOUND;
        };
        if incarnation != connected.incarnation {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        }
        let result = connected.input(&instance, session, bytes);
        drop(guard);
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
        if !instance
            .link
            .connect_terminal(&instance, session, Arc::clone(&subscription))
        {
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
        let Some(entry) = instance.state.subscription(session, id, generation) else {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        };
        entry.refresh.notify_one();
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
