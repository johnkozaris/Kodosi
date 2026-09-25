use super::{
    Arc, CALLBACK_DEPTH, CancellationToken, Condvar, KODOSI_FFI_OK, KODOSI_MAX_FRAME_BYTES,
    KODOSI_TERMINAL_CONTROL_MAX_BYTES, KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
    KodosiCallbacks, Mutex, MutexGuard, Uuid, c_void,
    subscriptions::{SubscriptionState, Subscriptions},
    terminal,
};

#[derive(Clone, Copy)]
pub(super) struct UserData(pub(super) *mut c_void);
unsafe impl Send for UserData {}
unsafe impl Sync for UserData {}
impl UserData {
    pub(super) fn get(self) -> *mut c_void {
        self.0
    }
}

pub(super) fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Default)]
pub(super) struct Gate {
    pub(super) state: Mutex<GateState>,
    idle: Condvar,
}
#[derive(Default)]
pub(super) struct GateState {
    closed: bool,
    pub(super) active: usize,
}
pub(super) struct Permit<'a>(&'a Gate);
impl Gate {
    pub(super) fn enter(&self) -> Option<Permit<'_>> {
        let mut state = lock(&self.state);
        if state.closed {
            return None;
        }
        state.active += 1;
        drop(state);
        Some(Permit(self))
    }
    pub(super) fn close(&self) {
        lock(&self.state).closed = true;
    }
    pub(super) fn wait(&self) {
        let mut state = lock(&self.state);
        while state.active != 0 {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(state);
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = lock(&self.0.state);
        state.active -= 1;
        if state.active == 0 {
            self.0.idle.notify_all();
        }
    }
}
pub(super) struct CallbackScope;
impl CallbackScope {
    pub(super) fn enter() -> Self {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
    pub(super) fn active() -> bool {
        CALLBACK_DEPTH.with(|depth| depth.get() != 0)
    }
}
impl Drop for CallbackScope {
    fn drop(&mut self) {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(super) struct State {
    pub(super) callbacks: KodosiCallbacks,
    pub(super) userdata: UserData,
    pub(super) gate: Gate,
    pub(super) serial: Mutex<()>,
    pub(super) cancel: CancellationToken,
    pub(super) subscriptions: Mutex<Subscriptions>,
}
impl State {
    pub(super) fn catalog(&self, snapshot: &serde_json::Value) {
        let Some(sessions) = snapshot
            .get("sessions")
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        let sessions = sessions
            .iter()
            .filter_map(|session| {
                session
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|id| Uuid::parse_str(id).ok())
            })
            .collect();
        lock(&self.subscriptions).catalog(sessions);
    }
    pub(super) fn remove(&self, session: Uuid, token: &Arc<SubscriptionState>) {
        let mut registry = lock(&self.subscriptions);
        if registry
            .entries
            .get(&session)
            .is_some_and(|entry| Arc::ptr_eq(entry, token))
        {
            registry.entries.remove(&session);
        }
        if !registry.sessions.contains(&session) && !registry.entries.contains_key(&session) {
            registry.generations.remove(&session);
        }
    }
    pub(super) fn callback<R>(
        &self,
        subscription: Option<&SubscriptionState>,
        action: impl FnOnce() -> R,
    ) -> Option<R> {
        let _serial = lock(&self.serial);
        let _permit = self.gate.enter()?;
        let _subscription = if let Some(subscription) = subscription {
            Some(subscription.gate.enter()?)
        } else {
            None
        };
        let _scope = CallbackScope::enter();
        Some(action())
    }
    pub(super) fn event(&self, event: &kodosi_runtime::Event) {
        let Ok(bytes) = serde_json::to_vec(event) else {
            return;
        };
        if bytes.len() > KODOSI_MAX_FRAME_BYTES {
            return;
        }
        let value = matches!(
            &event.event,
            kodosi_runtime::protocol::EventBody::SessionsSnapshot { .. }
        )
        .then(|| serde_json::to_value(event).ok())
        .flatten()
        .or_else(|| {
            event
                .event
                .kind()
                .starts_with("term.")
                .then(|| serde_json::to_value(event).ok())
                .flatten()
        });
        if let Some(value) = &value
            && value.get("type").and_then(serde_json::Value::as_str) == Some("sessions.snapshot")
        {
            self.catalog(value);
        }
        if let Some(value) = &value
            && value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind.starts_with("term.") && kind != "term.notification")
            && let Some(session) = value
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())
        {
            let entry = lock(&self.subscriptions).entries.get(&session).cloned();
            if let Some(entry) = entry {
                let matches = value
                    .get("subscriptionId")
                    .or_else(|| value.get("clientId"))
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|id| {
                        value
                            .get("subscriptionGeneration")
                            .and_then(serde_json::Value::as_u64)
                            .is_some_and(|generation| entry.matches(id, generation))
                    });
                if matches {
                    self.control(&entry, value);
                }
            }
        }
        if let Some(callback) = self.callbacks.on_event {
            self.callback(None, || unsafe {
                callback(bytes.as_ptr(), bytes.len(), self.userdata.get());
            });
        }
    }
    pub(super) fn control(&self, subscription: &SubscriptionState, payload: &serde_json::Value) {
        let Ok(bytes) = serde_json::to_vec(payload) else {
            return;
        };
        if bytes.len() > KODOSI_TERMINAL_CONTROL_MAX_BYTES {
            return;
        }
        if let Some(callback) = self.callbacks.on_terminal_control {
            self.callback(Some(subscription), || unsafe {
                callback(
                    subscription.session.as_ptr(),
                    subscription.id.as_ptr(),
                    subscription.generation,
                    bytes.as_ptr(),
                    bytes.len(),
                    self.userdata.get(),
                );
            });
        }
    }
    pub(super) fn result(&self, subscription: &SubscriptionState, result: i32) {
        if let Some(callback) = self.callbacks.on_terminal_connect_result {
            self.callback(Some(subscription), || unsafe {
                callback(
                    subscription.session.as_ptr(),
                    subscription.id.as_ptr(),
                    subscription.generation,
                    result,
                    self.userdata.get(),
                );
            });
        }
    }
    pub(super) fn checkpoint(
        &self,
        subscription: &SubscriptionState,
        checkpoint: &terminal::Checkpoint,
        next_sequence: u64,
    ) -> bool {
        let bytes = &checkpoint.semantic_checkpoint;
        if bytes.is_empty() || bytes.len() > KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES {
            return false;
        }
        self.callbacks.on_terminal_checkpoint.and_then(|callback| {
            self.callback(Some(subscription), || unsafe {
                callback(
                    subscription.session.as_ptr(),
                    subscription.id.as_ptr(),
                    subscription.generation,
                    next_sequence,
                    checkpoint.rows(),
                    checkpoint.cols(),
                    bytes.as_ptr(),
                    bytes.len(),
                    self.userdata.get(),
                )
            })
        }) == Some(KODOSI_FFI_OK)
    }
    pub(super) fn data(&self, subscription: &SubscriptionState, frame: &terminal::DataFrame) {
        if let Some(callback) = self.callbacks.on_terminal_data {
            self.callback(Some(subscription), || unsafe {
                callback(
                    subscription.session.as_ptr(),
                    subscription.id.as_ptr(),
                    subscription.generation,
                    frame.sequence,
                    frame.bytes.as_ptr(),
                    frame.bytes.len(),
                    self.userdata.get(),
                );
            });
        }
    }
    pub(super) fn close(&self) {
        self.gate.close();
        self.cancel.cancel();
        let entries: Vec<_> = lock(&self.subscriptions)
            .entries
            .drain()
            .map(|(_, value)| value)
            .collect();
        for entry in entries {
            entry.retire();
        }
    }
}
