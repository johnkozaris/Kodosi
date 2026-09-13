use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_void},
    panic::AssertUnwindSafe,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use kodosi_runtime::{CommandEnvelope, Config, Error, RuntimeHandle, terminal};
use tokio::{runtime::Runtime as Executor, task::JoinSet};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[cfg(test)]
mod lifecycle_tests;

pub const KODOSI_FFI_ABI_VERSION: u32 = 6;
pub const KODOSI_MAX_FRAME_BYTES: usize = 8_388_608;
pub const KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES: usize = 8_388_608;
pub const KODOSI_TERMINAL_CONTROL_MAX_BYTES: usize = 65_536;
pub const KODOSI_FFI_OK: i32 = 0;
pub const KODOSI_FFI_NULL_HANDLE: i32 = 2;
pub const KODOSI_FFI_DESER_FAILED: i32 = 3;
pub const KODOSI_FFI_PAYLOAD_TOO_LARGE: i32 = 4;
pub const KODOSI_FFI_RUNTIME_STOPPED: i32 = 5;
pub const KODOSI_FFI_BUSY: i32 = 6;
pub const KODOSI_FFI_SESSION_NOT_FOUND: i32 = 7;
pub const KODOSI_FFI_STALE_SUBSCRIPTION: i32 = 8;
pub const KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED: i32 = 9;
pub const KODOSI_FFI_REQUIRED_CALLBACK_MISSING: i32 = 10;
pub const KODOSI_FFI_PANIC: i32 = -1;

const MAX_SUBSCRIPTIONS: usize = 4_096;
const STOP_BUDGET: Duration = Duration::from_secs(10);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);
static HANDLES: OnceLock<Mutex<HashMap<usize, Arc<Instance>>>> = OnceLock::new();

thread_local! {
    static CALLBACK_DEPTH: Cell<usize> = const { Cell::new(0) };
}

type EventCb = Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>;
type TerminalDataCb = Option<
    unsafe extern "C" fn(*const c_char, *const c_char, u64, u64, *const u8, usize, *mut c_void),
>;
type TerminalControlCb =
    Option<unsafe extern "C" fn(*const c_char, *const c_char, u64, *const u8, usize, *mut c_void)>;
type TerminalConnectResultCb =
    Option<unsafe extern "C" fn(*const c_char, *const c_char, u64, i32, *mut c_void)>;
type TerminalCheckpointCb = Option<
    unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        u64,
        u64,
        u16,
        u16,
        *const u8,
        usize,
        *mut c_void,
    ) -> i32,
>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KodosiCallbacks {
    pub on_event: EventCb,
    pub on_terminal_data: TerminalDataCb,
    pub on_terminal_control: TerminalControlCb,
    pub on_terminal_connect_result: TerminalConnectResultCb,
    pub on_terminal_checkpoint: TerminalCheckpointCb,
}

#[derive(Clone, Copy)]
struct UserData(*mut c_void);
unsafe impl Send for UserData {}
unsafe impl Sync for UserData {}
impl UserData {
    fn get(self) -> *mut c_void {
        self.0
    }
}

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    idle: Condvar,
}
#[derive(Default)]
struct GateState {
    closed: bool,
    active: usize,
}
struct Permit<'a>(&'a Gate);
impl Gate {
    fn enter(&self) -> Option<Permit<'_>> {
        let mut state = lock(&self.state);
        if state.closed {
            return None;
        }
        state.active += 1;
        drop(state);
        Some(Permit(self))
    }
    fn close(&self) {
        lock(&self.state).closed = true;
    }
    fn wait(&self) {
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
struct CallbackScope;
impl CallbackScope {
    fn enter() -> Self {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
    fn active() -> bool {
        CALLBACK_DEPTH.with(|depth| depth.get() != 0)
    }
}
impl Drop for CallbackScope {
    fn drop(&mut self) {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

struct SubscriptionState {
    session: CString,
    id: CString,
    generation: u64,
    cancel: CancellationToken,
    refresh: tokio::sync::Notify,
    gate: Gate,
    connected: Mutex<Option<(Uuid, Uuid)>>,
}
impl SubscriptionState {
    fn matches(&self, id: &str, generation: u64) -> bool {
        self.id.as_bytes() == id.as_bytes() && self.generation == generation
    }
    fn retire(&self) {
        self.gate.close();
        self.cancel.cancel();
        if !CallbackScope::active() {
            self.gate.wait();
        }
    }
}
#[derive(Default)]
struct Subscriptions {
    entries: HashMap<Uuid, Arc<SubscriptionState>>,
    generations: HashMap<Uuid, u64>,
    sessions: HashSet<Uuid>,
}
impl Subscriptions {
    fn catalog(&mut self, sessions: HashSet<Uuid>) {
        self.sessions = sessions;
        self.generations
            .retain(|id, _| self.sessions.contains(id) || self.entries.contains_key(id));
    }
}

struct State {
    callbacks: KodosiCallbacks,
    userdata: UserData,
    gate: Gate,
    serial: Mutex<()>,
    cancel: CancellationToken,
    subscriptions: Mutex<Subscriptions>,
}
impl State {
    fn catalog(&self, snapshot: &serde_json::Value) {
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
    fn remove(&self, session: Uuid, token: &Arc<SubscriptionState>) {
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
    fn callback<R>(
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
    fn event(&self, event: &kodosi_runtime::Event) {
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
                .is_some_and(|kind| kind.starts_with("term."))
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
    fn control(&self, subscription: &SubscriptionState, payload: &serde_json::Value) {
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
    fn result(&self, subscription: &SubscriptionState, result: i32) {
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
    fn checkpoint(
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
    fn data(&self, subscription: &SubscriptionState, frame: &terminal::DataFrame) {
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
    fn close(&self) {
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
struct ActiveRuntime;
impl ActiveRuntime {
    fn acquire() -> Option<Self> {
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}
impl Drop for ActiveRuntime {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}
#[derive(Default)]
struct Completion {
    done: Mutex<bool>,
    changed: Condvar,
}
impl Completion {
    fn finish(&self) {
        *lock(&self.done) = true;
        self.changed.notify_all();
    }
    fn wait(&self) {
        let mut done = lock(&self.done);
        while !*done {
            done = self
                .changed
                .wait(done)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(done);
    }
}
struct Instance {
    runtime: RuntimeHandle,
    state: Arc<State>,
    executor: tokio::runtime::Handle,
    tasks: Mutex<Option<JoinSet<()>>>,
    stopped: Completion,
}
impl Instance {
    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> bool {
        let mut guard = lock(&self.tasks);
        let Some(tasks) = guard.as_mut() else {
            return false;
        };
        if self.state.cancel.is_cancelled() {
            return false;
        }
        while tasks.try_join_next().is_some() {}
        tasks.spawn_on(future, &self.executor);
        drop(guard);
        true
    }
}
fn handles() -> &'static Mutex<HashMap<usize, Arc<Instance>>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn instance(handle: *mut c_void) -> Option<Arc<Instance>> {
    if handle.is_null() {
        return None;
    }
    lock(handles())
        .get(&(handle as usize))
        .filter(|instance| !instance.state.cancel.is_cancelled())
        .cloned()
}
fn code(error: &Error) -> i32 {
    match error {
        Error::Busy => KODOSI_FFI_BUSY,
        Error::Stopped => KODOSI_FFI_RUNTIME_STOPPED,
        Error::NotFound => KODOSI_FFI_SESSION_NOT_FOUND,
        Error::Stale => KODOSI_FFI_STALE_SUBSCRIPTION,
        _ => KODOSI_FFI_DESER_FAILED,
    }
}
fn caught(action: impl FnOnce() -> i32) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(action)).unwrap_or(KODOSI_FFI_PANIC)
}
unsafe fn text<'a>(value: *const c_char) -> Option<&'a str> {
    if value.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(value) }.to_str().ok()?;
    (value.len() <= 256).then_some(value)
}
unsafe fn payload<'a>(bytes: *const u8, len: usize) -> Result<&'a [u8], i32> {
    if len > KODOSI_MAX_FRAME_BYTES {
        return Err(KODOSI_FFI_PAYLOAD_TOO_LARGE);
    }
    if bytes.is_null() {
        return if len == 0 {
            Ok(&[])
        } else {
            Err(KODOSI_FFI_DESER_FAILED)
        };
    }
    Ok(unsafe { std::slice::from_raw_parts(bytes, len) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_start(
    callbacks: *const KodosiCallbacks,
    callbacks_size: usize,
    userdata: *mut c_void,
) -> *mut c_void {
    let started = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if callbacks.is_null() || callbacks_size != std::mem::size_of::<KodosiCallbacks>() {
            return None;
        }
        let callbacks = unsafe { callbacks.read() };
        if callbacks.on_event.is_none()
            || callbacks.on_terminal_data.is_none()
            || callbacks.on_terminal_control.is_none()
            || callbacks.on_terminal_connect_result.is_none()
            || callbacks.on_terminal_checkpoint.is_none()
        {
            return None;
        }
        let active = ActiveRuntime::acquire()?;
        let key = NEXT_HANDLE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .ok()?;
        let (ready, receive) = std::sync::mpsc::sync_channel(1);
        let userdata = UserData(userdata);
        std::thread::Builder::new()
            .name("kodosi-runtime".into())
            .spawn(move || run_executor(key, callbacks, userdata, ready, active))
            .ok()?;
        let (instance, initial, events) = receive.recv().ok()??;
        instance.spawn(pump_events(
            instance.runtime.clone(),
            Arc::clone(&instance.state),
            initial,
            events,
        ));
        Some(key as *mut c_void)
    }));
    match started {
        Ok(Some(handle)) => handle,
        _ => std::ptr::null_mut(),
    }
}

type Started = (
    Arc<Instance>,
    Vec<kodosi_runtime::Event>,
    tokio::sync::broadcast::Receiver<kodosi_runtime::Event>,
);

fn run_executor(
    key: usize,
    callbacks: KodosiCallbacks,
    userdata: UserData,
    ready: std::sync::mpsc::SyncSender<Option<Started>>,
    active: ActiveRuntime,
) {
    let setup = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let executor = Executor::new().ok()?;
        let instance = Arc::new(Instance {
            runtime: match Config::load()
                .and_then(|config| executor.block_on(kodosi_runtime::start(config)))
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::error!(%error, "could not start embedded runtime");
                    return None;
                }
            },
            state: Arc::new(State {
                callbacks,
                userdata,
                gate: Gate::default(),
                serial: Mutex::new(()),
                cancel: CancellationToken::new(),
                subscriptions: Mutex::new(Subscriptions::default()),
            }),
            executor: executor.handle().clone(),
            tasks: Mutex::new(Some(JoinSet::new())),
            stopped: Completion::default(),
        });
        let (initial, events) = match executor.block_on(instance.runtime.observe()) {
            Ok(observation) => observation,
            Err(error) => {
                tracing::error!(%error, "could not observe embedded runtime");
                return None;
            }
        };
        Some((executor, instance, initial, events))
    }));
    let Ok(Some((executor, instance, initial, events))) = setup else {
        drop(active);
        let _ = ready.send(None);
        return;
    };
    lock(handles()).insert(key, Arc::clone(&instance));
    if ready
        .send(Some((Arc::clone(&instance), initial, events)))
        .is_err()
    {
        instance.state.close();
    }
    drop(ready);
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        executor.block_on(async {
            tokio::select! {
                () = instance.state.cancel.cancelled() => {},
                () = instance.runtime.stopped() => {},
            }
        });
        instance.state.close();
        instance.state.gate.wait();
        executor.block_on(async {
            let _ = tokio::time::timeout(STOP_BUDGET, instance.runtime.shutdown()).await;
            let tasks = lock(&instance.tasks).take();
            if let Some(mut tasks) = tasks {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
            }
        });
    }));
    instance.state.gate.close();
    instance.state.gate.wait();
    executor.shutdown_timeout(STOP_BUDGET);
    lock(handles()).remove(&key);
    drop(active);
    instance.stopped.finish();
}

async fn pump_events(
    runtime: RuntimeHandle,
    state: Arc<State>,
    initial: Vec<kodosi_runtime::Event>,
    mut events: tokio::sync::broadcast::Receiver<kodosi_runtime::Event>,
) {
    for event in initial {
        state.event(&event);
    }
    loop {
        tokio::select! {
            biased;
            () = state.cancel.cancelled() => break,
            event = events.recv() => match event {
                Ok(event) => state.event(&event),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    match runtime.observe().await {
                        Ok((snapshot, receiver)) => {
                            events = receiver;
                            for event in snapshot { state.event(&event); }
                        }
                        Err(_) => break,
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_stop(handle: *mut c_void) {
    let Some(instance) = ({ lock(handles()).get(&(handle as usize)).cloned() }) else {
        return;
    };
    instance.state.close();
    if !CallbackScope::active() {
        instance.state.gate.wait();
        instance.stopped.wait();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_abi_version() -> u32 {
    KODOSI_FFI_ABI_VERSION
}
#[unsafe(no_mangle)]
pub extern "C" fn kodosi_protocol_version() -> u32 {
    kodosi_runtime::protocol::VERSION
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_command(
    handle: *mut c_void,
    bytes: *const u8,
    len: usize,
) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        if len > kodosi_runtime::protocol::MAX_COMMAND_BYTES {
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }
        let bytes = match unsafe { payload(bytes, len) } {
            Ok(value) => value,
            Err(error) => return error,
        };
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => return KODOSI_FFI_DESER_FAILED,
        };
        if matches!(
            value.get("type").and_then(serde_json::Value::as_str),
            Some("session.resize" | "session.focus" | "session.blur")
        ) {
            let Some(session) = value
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
            else {
                return KODOSI_FFI_DESER_FAILED;
            };
            let Some(id) = value
                .get("subscriptionId")
                .or_else(|| value.get("clientId"))
                .and_then(serde_json::Value::as_str)
            else {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            };
            let Some(generation) = value
                .get("subscriptionGeneration")
                .and_then(serde_json::Value::as_u64)
            else {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            };
            let Some(incarnation) = value
                .get("expectedRuntimeIncarnationId")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())
            else {
                return KODOSI_FFI_DESER_FAILED;
            };
            let registry = lock(&instance.state.subscriptions);
            let Some(subscription) = registry
                .entries
                .get(&session)
                .filter(|s| s.matches(id, generation) && !s.cancel.is_cancelled())
            else {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            };
            let Some((connection, current)) = *lock(&subscription.connected) else {
                return KODOSI_FFI_SESSION_NOT_FOUND;
            };
            if current != incarnation {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            }
            let command: CommandEnvelope = match serde_json::from_value(value) {
                Ok(value) => value,
                Err(_) => return KODOSI_FFI_DESER_FAILED,
            };
            let result = instance
                .runtime
                .try_send_terminal(command, connection)
                .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK);
            drop(registry);
            return result;
        }
        let command: CommandEnvelope = match serde_json::from_value(value) {
            Ok(value) => value,
            Err(_) => return KODOSI_FFI_DESER_FAILED,
        };
        instance
            .runtime
            .try_send(command)
            .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK)
    })
}

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
    let mut next = subscriber.next_sequence;
    let mut pending = std::collections::VecDeque::new();
    let mut seed: Option<kodosi_runtime::network::CheckpointCut> = None;
    let mut closed: Option<(String, u64)> = None;
    loop {
        if let Some(cut) = seed.as_ref() {
            if cut.next_sequence < next {
                closed = Some(("terminal refresh snapshot regressed".into(), next));
                seed = None;
            } else if cut.next_sequence == next {
                if !state.checkpoint(token, &cut.checkpoint, cut.next_sequence) {
                    closed = Some(("the terminal snapshot could not be restored".into(), next));
                }
                seed = None;
            }
        }
        while let Some(&(rows, cols, at_sequence)) = pending.front() {
            if at_sequence > next {
                break;
            }
            pending.pop_front();
            if at_sequence != next {
                closed = Some(("terminal resize arrived out of order".into(), next));
                break;
            }
            state.control(token, &serde_json::json!({"type":"term.resize","rows":rows,"cols":cols,"atSequence":at_sequence}));
        }
        if let Some((reason, final_sequence)) = &closed
            && *final_sequence <= next
        {
            state.control(
                token,
                &serde_json::json!({"type":"term.closed","reason":reason,"finalSequence":next}),
            );
            break;
        }
        tokio::select! {
            biased;
            () = token.cancel.cancelled() => break,
            () = token.refresh.notified(), if closed.is_none() && seed.is_none() => {
                let fresh = tokio::select! {
                    biased;
                    () = token.cancel.cancelled() => break,
                    result = runtime.terminal_checkpoint(session, subscriber.connection_id) => result,
                };
                match fresh {
                    Ok(cut) => seed = Some(cut),
                    Err(error) => closed = Some((error.to_string(), next)),
                }
            }
            control = subscriber.control.recv(), if closed.is_none() => match control {
                Some(terminal::ControlFrame::Resize { rows, cols, at_sequence }) => {
                    if pending.len() >= 32 || pending.back().is_some_and(|&(_, _, last)| last > at_sequence) {
                        closed = Some(("terminal resize queue exceeded its ordering bound".into(), next));
                    } else { pending.push_back((rows, cols, at_sequence)); }
                }
                Some(terminal::ControlFrame::Closed { reason, final_sequence }) => closed = Some((reason, final_sequence)),
                None => closed = Some(("terminal disconnected".into(), next)),
            },
            data = subscriber.data.recv() => match data {
                Some(frame) if frame.sequence < next => {},
                Some(frame) if frame.sequence == next => {
                    if let Some(sequence) = next.checked_add(1) { state.data(token, &frame); next = sequence; }
                    else { closed = Some(("terminal output sequence exhausted".into(), next)); }
                }
                Some(_) => closed = Some(("terminal output needs a fresh snapshot".into(), next)),
                None => {
                    let reason = closed.take().map_or_else(|| "terminal disconnected".into(), |(reason, _)| reason);
                    state.control(token, &serde_json::json!({"type":"term.closed","reason":reason,"finalSequence":next}));
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    #[test]
    fn opaque_unknown_and_stopped_handles_are_not_dereferenced() {
        assert!(instance(std::ptr::null_mut()).is_none());
        assert!(instance(usize::MAX as *mut c_void).is_none());
        unsafe {
            kodosi_stop(usize::MAX as *mut c_void);
        }
    }
    #[test]
    fn closed_gate_rejects_future_callbacks() {
        let gate = Gate::default();
        let permit = gate.enter().expect("open gate");
        gate.close();
        assert!(gate.enter().is_none());
        drop(permit);
        gate.wait();
    }
    #[test]
    fn subscription_identity_is_exact() {
        let token = SubscriptionState {
            session: CString::new("session").unwrap(),
            id: CString::new("subscriber").unwrap(),
            generation: 7,
            cancel: CancellationToken::new(),
            refresh: tokio::sync::Notify::new(),
            gate: Gate::default(),
            connected: Mutex::new(None),
        };
        assert!(token.matches("subscriber", 7));
        assert!(!token.matches("subscriber", 6));
        assert!(!token.matches("other", 7));
        token.retire();
        assert!(token.cancel.is_cancelled());
        assert!(token.gate.enter().is_none());
    }
    #[test]
    fn start_rejects_missing_or_wrong_callback_table() {
        assert!(unsafe { kodosi_start(std::ptr::null(), 0, std::ptr::null_mut()) }.is_null());
        let callbacks = KodosiCallbacks {
            on_event: None,
            on_terminal_data: None,
            on_terminal_control: None,
            on_terminal_connect_result: None,
            on_terminal_checkpoint: None,
        };
        assert!(
            unsafe {
                kodosi_start(
                    &raw const callbacks,
                    std::mem::size_of::<KodosiCallbacks>(),
                    std::ptr::null_mut(),
                )
            }
            .is_null()
        );
    }
    #[test]
    fn callback_shutdown_waits_for_inflight_borrowed_userdata() {
        let gate = Arc::new(Gate::default());
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let callback_gate = Arc::clone(&gate);
        let callback_barrier = Arc::clone(&barrier);
        let worker = std::thread::spawn(move || {
            let _permit = callback_gate.enter().unwrap();
            callback_barrier.wait();
            std::thread::sleep(Duration::from_millis(20));
        });
        barrier.wait();
        gate.close();
        gate.wait();
        assert_eq!(lock(&gate.state).active, 0);
        assert!(gate.enter().is_none());
        worker.join().unwrap();
    }
    unsafe extern "C" fn ignore_event(_: *const u8, _: usize, _: *mut c_void) {}
    unsafe extern "C" fn ignore_data(
        _: *const c_char,
        _: *const c_char,
        _: u64,
        _: u64,
        _: *const u8,
        _: usize,
        _: *mut c_void,
    ) {
    }
    unsafe extern "C" fn ignore_control(
        _: *const c_char,
        _: *const c_char,
        _: u64,
        _: *const u8,
        _: usize,
        _: *mut c_void,
    ) {
    }
    unsafe extern "C" fn ignore_result(
        _: *const c_char,
        _: *const c_char,
        _: u64,
        _: i32,
        _: *mut c_void,
    ) {
    }
    unsafe extern "C" fn accept_checkpoint(
        _: *const c_char,
        _: *const c_char,
        _: u64,
        _: u64,
        _: u16,
        _: u16,
        _: *const u8,
        _: usize,
        _: *mut c_void,
    ) -> i32 {
        KODOSI_FFI_OK
    }
    fn callbacks() -> KodosiCallbacks {
        KodosiCallbacks {
            on_event: Some(ignore_event),
            on_terminal_data: Some(ignore_data),
            on_terminal_control: Some(ignore_control),
            on_terminal_connect_result: Some(ignore_result),
            on_terminal_checkpoint: Some(accept_checkpoint),
        }
    }
    fn state() -> State {
        State {
            callbacks: callbacks(),
            userdata: UserData(std::ptr::null_mut()),
            gate: Gate::default(),
            serial: Mutex::new(()),
            cancel: CancellationToken::new(),
            subscriptions: Mutex::new(Subscriptions::default()),
        }
    }
    fn subscription(session: Uuid, generation: u64) -> Arc<SubscriptionState> {
        Arc::new(SubscriptionState {
            session: CString::new(session.to_string()).unwrap(),
            id: CString::new("view").unwrap(),
            generation,
            cancel: CancellationToken::new(),
            refresh: tokio::sync::Notify::new(),
            gate: Gate::default(),
            connected: Mutex::new(None),
        })
    }
    #[test]
    fn completed_sessions_do_not_accumulate_subscription_generations() {
        let state = state();
        for _ in 0..=MAX_SUBSCRIPTIONS {
            let session = Uuid::now_v7();
            let token = subscription(session, 1);
            {
                let mut registry = lock(&state.subscriptions);
                registry.catalog(HashSet::from([session]));
                registry.generations.insert(session, 1);
                registry.entries.insert(session, Arc::clone(&token));
                registry.catalog(HashSet::new());
                assert!(registry.generations.contains_key(&session));
                drop(registry);
            }
            state.remove(session, &token);
            assert!(lock(&state.subscriptions).generations.is_empty());
        }
    }
    #[test]
    fn old_subscription_completion_cannot_remove_a_replacement() {
        let state = state();
        let session = Uuid::now_v7();
        let old = subscription(session, 1);
        let current = subscription(session, 2);
        lock(&state.subscriptions)
            .entries
            .insert(session, Arc::clone(&current));
        state.remove(session, &old);
        assert!(Arc::ptr_eq(
            lock(&state.subscriptions).entries.get(&session).unwrap(),
            &current
        ));
    }
    #[test]
    fn concurrent_stop_waiters_share_completion() {
        let done = Arc::new(Completion::default());
        let mut threads = Vec::new();
        for _ in 0..4 {
            let done = Arc::clone(&done);
            threads.push(std::thread::spawn(move || {
                done.wait();
                assert!(*lock(&done.done));
            }));
        }
        done.finish();
        for thread in threads {
            thread.join().unwrap();
        }
        done.wait();
    }
    #[test]
    fn callbacks_can_retire_their_own_subscription_without_deadlock() {
        let state = state();
        let token = subscription(Uuid::now_v7(), 1);
        assert!(state.callback(Some(&token), || token.retire()).is_some());
        assert!(state.callback(Some(&token), || ()).is_none());
        token.gate.wait();
    }
    #[test]
    fn callbacks_cannot_reopen_closed_admission() {
        let gate = Gate::default();
        let _scope = CallbackScope::enter();
        assert!(CallbackScope::active());
        gate.close();
        assert!(gate.enter().is_none());
    }
}
