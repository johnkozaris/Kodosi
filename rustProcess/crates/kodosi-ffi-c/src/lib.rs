use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::{CStr, c_char, c_void};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex, MutexGuard};
use std::time::{Duration, Instant};

fn lock_poison_ok<T>(m: &StdMutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

use tokio::runtime::Runtime as TokioRuntime;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use kodosi_runtime::terminal_transport::{TerminalCapability, TerminalSurface};
use kodosi_runtime::{
    AgentIntelCommand, AuthCommand, DeviceCommand, EmbeddedRuntime, FriendsCommand, RoomCommand,
    RuntimeCommandSink, RuntimeEventReceivers, SessionCommand, SystemCommand, TerminalCommand,
    TrustCommand,
};

pub const KODOSI_FFI_ABI_VERSION: u32 = 5;

pub const KODOSI_MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub const KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES: usize = 8_388_608;

pub const KODOSI_TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES: usize = 8_388_608;

pub const KODOSI_TERMINAL_CONTROL_MAX_BYTES: usize = 8_454_144;

const _: () = assert!(
    KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES
        == kodosi_domain::terminal::TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES
);
const _: () = assert!(
    KODOSI_TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES
        == kodosi_domain::terminal::TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES
);
const _: () = assert!(
    KODOSI_TERMINAL_CONTROL_MAX_BYTES
        == kodosi_domain::terminal::TERMINAL_PRESENTATION_CONTROL_MAX_BYTES
);

const RUNTIME_STOPPED_STATE: u8 = 0;
const RUNTIME_RUNNING_STATE: u8 = 1;
const RUNTIME_STOPPING_STATE: u8 = 2;
static RUNTIME_STATE: AtomicU8 = AtomicU8::new(RUNTIME_STOPPED_STATE);

const FFI_STOP_GRACE: Duration = Duration::from_secs(5);

thread_local! {


    static CALLBACK_DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct CallbackScope;

impl CallbackScope {
    fn enter() -> Self {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }

    fn is_active() -> bool {
        CALLBACK_DEPTH.with(|depth| depth.get() != 0)
    }
}

impl Drop for CallbackScope {
    fn drop(&mut self) {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn try_open_lifecycle_gate() -> bool {
    RUNTIME_STATE
        .compare_exchange(
            RUNTIME_STOPPED_STATE,
            RUNTIME_RUNNING_STATE,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

type EventCb = Option<unsafe extern "C" fn(json: *const u8, len: usize, ud: *mut c_void)>;

type TerminalDataV2Cb = Option<
    unsafe extern "C" fn(
        session_id: *const c_char,
        subscription_id: *const c_char,
        subscription_generation: u64,
        sequence: u64,
        bytes: *const u8,
        len: usize,
        ud: *mut c_void,
    ),
>;

type TerminalControlV2Cb = Option<
    unsafe extern "C" fn(
        session_id: *const c_char,
        subscription_id: *const c_char,
        subscription_generation: u64,
        json: *const u8,
        len: usize,
        ud: *mut c_void,
    ),
>;

type TerminalConnectResultV2Cb = Option<
    unsafe extern "C" fn(
        session_id: *const c_char,
        subscription_id: *const c_char,
        subscription_generation: u64,
        result: i32,
        ud: *mut c_void,
    ),
>;

type TerminalSemanticCheckpointV2Cb = Option<
    unsafe extern "C" fn(
        session_id: *const c_char,
        subscription_id: *const c_char,
        subscription_generation: u64,
        next_sequence: u64,
        rows: u16,
        cols: u16,
        semantic_json: *const u8,
        semantic_json_len: usize,
        ud: *mut c_void,
    ) -> i32,
>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KodosiCallbacks {
    pub on_auth_event: EventCb,
    pub on_session_event: EventCb,
    pub on_system_event: EventCb,
    pub on_friends_event: EventCb,
    pub on_devices_event: EventCb,
    pub on_agent_intel_event: EventCb,
    pub on_agent_global_event: EventCb,
    pub on_trust_event: EventCb,
    pub on_room_event: EventCb,
    pub on_terminal_data_v2: TerminalDataV2Cb,
    pub on_terminal_control_v2: TerminalControlV2Cb,
    pub on_terminal_connect_result_v2: TerminalConnectResultV2Cb,
    pub on_terminal_semantic_checkpoint_v2: TerminalSemanticCheckpointV2Cb,
}

unsafe impl Send for KodosiCallbacks {}

unsafe impl Sync for KodosiCallbacks {}

#[derive(Clone, Copy)]
struct SendPtr(*mut c_void);

unsafe impl Send for SendPtr {}

unsafe impl Sync for SendPtr {}

impl SendPtr {
    fn raw(self) -> *mut c_void {
        self.0
    }
}

struct SendCallbackCtx {
    ud: SendPtr,
    admission: Arc<CallbackAdmission>,
    c_sid: std::ffi::CString,
    c_subscription_id: std::ffi::CString,
    subscription_generation: u64,
}

unsafe impl Send for SendCallbackCtx {}

struct FfiRuntime {
    tokio_rt: TokioRuntime,
    embedded: Mutex<Option<EmbeddedRuntime>>,
    command_sink: StdMutex<Option<RuntimeCommandSink>>,
    callbacks: KodosiCallbacks,
    userdata: SendPtr,
    callback_admission: Arc<CallbackAdmission>,
    terminal_connections: TerminalConnections,
    terminal_connect_result_join: StdMutex<Option<tokio::task::JoinHandle<()>>>,
    spawned: StdMutex<Option<JoinSet<()>>>,
}

struct CallbackAdmission {
    state: StdMutex<CallbackAdmissionState>,
    idle: Condvar,
}

struct CallbackAdmissionState {
    open: bool,
    active: usize,
}

struct CallbackPermit<'a> {
    admission: &'a CallbackAdmission,
    _scope: CallbackScope,
}

impl CallbackAdmission {
    fn open() -> Self {
        Self {
            state: StdMutex::new(CallbackAdmissionState {
                open: true,
                active: 0,
            }),
            idle: Condvar::new(),
        }
    }

    fn enter(&self) -> Option<CallbackPermit<'_>> {
        let mut state = lock_poison_ok(&self.state);
        if !state.open {
            return None;
        }
        state.active = state.active.saturating_add(1);
        drop(state);
        Some(CallbackPermit {
            admission: self,
            _scope: CallbackScope::enter(),
        })
    }

    fn enter_reserved(&self) -> CallbackPermit<'_> {
        let mut state = lock_poison_ok(&self.state);
        state.active = state.active.saturating_add(1);
        drop(state);
        CallbackPermit {
            admission: self,
            _scope: CallbackScope::enter(),
        }
    }

    fn close(&self) {
        lock_poison_ok(&self.state).open = false;
    }

    fn wait_until_idle(&self, deadline: Instant) -> bool {
        let mut state = lock_poison_ok(&self.state);
        while state.active != 0 {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let result = self.idle.wait_timeout(state, remaining);
            let (next, timeout) = result.unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next;
            if timeout.timed_out() && state.active != 0 {
                return false;
            }
        }
        true
    }
}

impl Drop for CallbackPermit<'_> {
    fn drop(&mut self) {
        let mut state = lock_poison_ok(&self.admission.state);
        state.active = state.active.saturating_sub(1);
        if state.active == 0 {
            self.admission.idle.notify_all();
        }
    }
}

struct TerminalConnections {
    inner: StdMutex<TerminalConnectionsInner>,
}

struct TerminalConnectionsInner {
    registration_open: bool,
    next_internal_epoch: u64,
    states: std::collections::HashMap<String, TerminalConnectionState>,
    last_external_generations: std::collections::HashMap<String, u64>,
    callback_tx: Option<tokio::sync::mpsc::UnboundedSender<TerminalConnectResult>>,
}

struct TerminalConnectionState {
    generation: u64,
    subscription: TerminalSubscription,
    cancellation: Option<tokio_util::sync::CancellationToken>,
    refresh: Option<Arc<tokio::sync::Notify>>,
    refresh_requested: bool,
    completion_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalSubscription {
    id: String,
    generation: u64,
}

struct TerminalConnectResult {
    session_id: String,
    subscription: TerminalSubscription,
    result: i32,
    delivered: Option<tokio::sync::oneshot::Sender<()>>,
}

struct TerminalConnectCompletion {
    tx: tokio::sync::mpsc::UnboundedSender<TerminalConnectResult>,
    event: TerminalConnectResult,
}

impl Default for TerminalConnections {
    fn default() -> Self {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        Self::new(tx)
    }
}

impl TerminalConnections {
    fn new(callback_tx: tokio::sync::mpsc::UnboundedSender<TerminalConnectResult>) -> Self {
        Self {
            inner: StdMutex::new(TerminalConnectionsInner {
                registration_open: true,
                next_internal_epoch: 0,
                states: std::collections::HashMap::new(),
                last_external_generations: std::collections::HashMap::new(),
                callback_tx: Some(callback_tx),
            }),
        }
    }

    fn reserve_completion(
        inner: &TerminalConnectionsInner,
        session_id: &str,
        subscription: TerminalSubscription,
        result: i32,
    ) -> Option<TerminalConnectCompletion> {
        Some(TerminalConnectCompletion {
            tx: inner.callback_tx.as_ref()?.clone(),
            event: TerminalConnectResult {
                session_id: session_id.to_owned(),
                subscription,
                result,
                delivered: None,
            },
        })
    }

    fn begin_connect(
        &self,
        session_id: &str,
        subscription: TerminalSubscription,
    ) -> Option<(u64, TerminalSubscription, Option<TerminalConnectCompletion>)> {
        let mut inner = lock_poison_ok(&self.inner);
        if !inner.registration_open {
            return None;
        }
        if inner
            .last_external_generations
            .get(session_id)
            .is_some_and(|last| subscription.generation <= *last)
        {
            return None;
        }
        inner
            .last_external_generations
            .insert(session_id.to_owned(), subscription.generation);
        inner.next_internal_epoch = inner.next_internal_epoch.wrapping_add(1);
        let generation = inner.next_internal_epoch;
        let prior = inner.states.insert(
            session_id.to_owned(),
            TerminalConnectionState {
                generation,
                subscription: subscription.clone(),
                cancellation: None,
                refresh: None,
                refresh_requested: false,
                completion_pending: true,
            },
        );
        let superseded_completion = prior
            .as_ref()
            .filter(|state| state.completion_pending)
            .and_then(|state| {
                Self::reserve_completion(
                    &inner,
                    session_id,
                    state.subscription.clone(),
                    KODOSI_FFI_STALE_SUBSCRIPTION,
                )
            });
        let prior_cancellation = prior.and_then(|state| state.cancellation);
        drop(inner);
        if let Some(prior) = prior_cancellation {
            prior.cancel();
        }
        Some((generation, subscription, superseded_completion))
    }

    fn registration_is_open(&self) -> bool {
        lock_poison_ok(&self.inner).registration_open
    }

    fn abandon_unspawned_connect(&self, session_id: &str, generation: u64) -> bool {
        let mut inner = lock_poison_ok(&self.inner);
        let completion_claimed = if inner
            .states
            .get(session_id)
            .is_some_and(|state| state.generation == generation)
        {
            inner.states.remove(session_id);
            false
        } else {
            true
        };
        drop(inner);
        completion_claimed
    }

    fn install(
        &self,
        session_id: &str,
        generation: u64,
        cancellation: tokio_util::sync::CancellationToken,
        refresh: Arc<tokio::sync::Notify>,
    ) -> bool {
        let mut inner = lock_poison_ok(&self.inner);
        let Some(state) = inner.states.get_mut(session_id) else {
            return false;
        };
        if state.generation != generation {
            return false;
        }
        state.cancellation = Some(cancellation);
        let refresh_requested = std::mem::take(&mut state.refresh_requested);
        let pending_refresh = refresh_requested.then(|| Arc::clone(&refresh));
        state.refresh = Some(refresh);
        drop(inner);
        if let Some(refresh) = pending_refresh {
            refresh.notify_one();
        }
        true
    }

    fn request_refresh(&self, session_id: &str, subscription: &TerminalSubscription) -> bool {
        let mut inner = lock_poison_ok(&self.inner);
        let Some(state) = inner.states.get_mut(session_id) else {
            return false;
        };
        if state.subscription != *subscription {
            return false;
        }
        if let Some(refresh) = state.refresh.clone() {
            drop(inner);
            refresh.notify_one();
        } else {
            state.refresh_requested = true;
        }
        true
    }

    fn disconnect(
        &self,
        session_id: &str,
        subscription: &TerminalSubscription,
    ) -> (bool, Option<TerminalConnectCompletion>) {
        let mut inner = lock_poison_ok(&self.inner);
        if inner
            .states
            .get(session_id)
            .is_none_or(|state| state.subscription != *subscription)
        {
            return (false, None);
        }
        let removed = inner.states.remove(session_id);
        let existed = removed.is_some();
        let pending = removed
            .as_ref()
            .filter(|state| state.completion_pending)
            .and_then(|state| {
                Self::reserve_completion(
                    &inner,
                    session_id,
                    state.subscription.clone(),
                    KODOSI_FFI_SESSION_NOT_FOUND,
                )
            });
        drop(inner);
        if let Some(token) = removed.and_then(|state| state.cancellation) {
            token.cancel();
        }
        (existed, pending)
    }

    fn clear_if_current(&self, session_id: &str, generation: u64) {
        let mut inner = lock_poison_ok(&self.inner);
        if inner
            .states
            .get(session_id)
            .is_some_and(|state| state.generation == generation)
        {
            inner.states.remove(session_id);
        }
    }

    fn finish_connect_if_current(
        &self,
        session_id: &str,
        generation: u64,
    ) -> Option<TerminalConnectCompletion> {
        let mut inner = lock_poison_ok(&self.inner);
        let state = inner.states.get_mut(session_id)?;
        if state.generation != generation || !state.completion_pending {
            return None;
        }
        state.completion_pending = false;
        let subscription = state.subscription.clone();
        let completion = Self::reserve_completion(&inner, session_id, subscription, KODOSI_FFI_OK);
        drop(inner);
        completion
    }

    fn fail_connect_if_current(
        &self,
        session_id: &str,
        generation: u64,
        result: i32,
    ) -> Option<TerminalConnectCompletion> {
        let mut inner = lock_poison_ok(&self.inner);
        if inner
            .states
            .get(session_id)
            .is_none_or(|state| state.generation != generation)
        {
            return None;
        }
        let removed = inner.states.remove(session_id)?;
        let completion = if removed.completion_pending {
            Self::reserve_completion(&inner, session_id, removed.subscription.clone(), result)
        } else {
            None
        };
        drop(inner);
        if let Some(cancellation) = removed.cancellation {
            cancellation.cancel();
        }
        completion
    }

    #[expect(
        clippy::significant_drop_tightening,
        reason = "The subscription lock must remain held through queue admission so replacement cannot race a validated resize."
    )]
    fn with_current_subscription<R>(
        &self,
        session_id: &str,
        subscription: &TerminalSubscription,
        send: impl FnOnce() -> R,
    ) -> Option<R> {
        let inner = lock_poison_ok(&self.inner);
        if !inner.registration_open
            || inner
                .states
                .get(session_id)
                .is_none_or(|state| state.subscription != *subscription)
        {
            return None;
        }
        Some(send())
    }

    fn current_subscription(&self, session_id: &str) -> Option<TerminalSubscription> {
        lock_poison_ok(&self.inner)
            .states
            .get(session_id)
            .map(|state| state.subscription.clone())
    }

    fn close_and_cancel_all(&self) -> Vec<TerminalConnectCompletion> {
        let mut inner = lock_poison_ok(&self.inner);
        inner.registration_open = false;
        let states = inner.states.drain().collect::<Vec<_>>();
        inner.last_external_generations.clear();
        let pending = states
            .iter()
            .filter(|(_, state)| state.completion_pending)
            .filter_map(|(session_id, state)| {
                Self::reserve_completion(
                    &inner,
                    session_id,
                    state.subscription.clone(),
                    KODOSI_FFI_RUNTIME_STOPPED,
                )
            })
            .collect();
        drop(inner.callback_tx.take());
        drop(inner);
        let cancellations = states
            .into_iter()
            .filter_map(|(_, state)| state.cancellation)
            .collect::<Vec<_>>();
        for cancellation in cancellations {
            cancellation.cancel();
        }
        pending
    }
}

struct FfiHandle {
    runtime: StdMutex<Option<Arc<FfiRuntime>>>,
}

fn runtime_from_handle(handle: *mut c_void) -> Option<Arc<FfiRuntime>> {
    if handle.is_null() {
        return None;
    }

    let handle = unsafe { &*(handle.cast::<FfiHandle>()) };
    lock_poison_ok(&handle.runtime).clone()
}

fn take_runtime_from_handle(handle: *mut c_void) -> Option<Arc<FfiRuntime>> {
    if handle.is_null() {
        return None;
    }

    let handle = unsafe { &*(handle.cast::<FfiHandle>()) };
    lock_poison_ok(&handle.runtime).take()
}

fn restore_runtime_to_handle(handle: *mut c_void, runtime: Arc<FfiRuntime>) {
    let handle = unsafe { &*(handle.cast::<FfiHandle>()) };
    *lock_poison_ok(&handle.runtime) = Some(runtime);
}

impl FfiRuntime {
    fn with_command_sink<R>(&self, f: impl FnOnce(&RuntimeCommandSink) -> R) -> Option<R> {
        lock_poison_ok(&self.command_sink).as_ref().map(f)
    }

    fn command_sink_clone(&self) -> Option<RuntimeCommandSink> {
        lock_poison_ok(&self.command_sink).clone()
    }
}

#[allow(
    clippy::significant_drop_tightening,
    reason = "The lock MUST be held across spawn_on — that's the soundness gate. \
              `kodosi_stop` takes the JoinSet under this same lock, so any spawn \
              that observes Some has its task tracked before stop can drain."
)]
fn try_spawn_tracked<F>(runtime: &Arc<FfiRuntime>, future: F) -> bool
where
    F: Future<Output = ()> + Send + 'static,
{
    let mut guard = lock_poison_ok(&runtime.spawned);
    let Some(set) = guard.as_mut() else {
        return false;
    };
    while let Some(result) = set.try_join_next() {
        if let Err(error) = result
            && error.is_panic()
        {
            tracing::warn!(?error, "FFI: completed tracked task panicked");
        }
    }
    set.spawn_on(future, runtime.tokio_rt.handle());
    true
}

unsafe impl Send for FfiRuntime {}

unsafe impl Sync for FfiRuntime {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_start_v2(
    callbacks: *const KodosiCallbacks,
    callbacks_size: usize,
    userdata: *mut c_void,
) -> *mut c_void {
    let Some(callbacks) = (unsafe { decode_callbacks_v2(callbacks, callbacks_size) }) else {
        return std::ptr::null_mut();
    };
    start_runtime(callbacks, userdata)
}

unsafe fn decode_callbacks_v2(
    callbacks: *const KodosiCallbacks,
    callbacks_size: usize,
) -> Option<KodosiCallbacks> {
    if callbacks.is_null() || callbacks_size != std::mem::size_of::<KodosiCallbacks>() {
        return None;
    }

    let callback_table = unsafe { callbacks.read() };
    if callback_table.on_terminal_data_v2.is_none()
        || callback_table.on_terminal_control_v2.is_none()
        || callback_table.on_terminal_connect_result_v2.is_none()
        || callback_table.on_terminal_semantic_checkpoint_v2.is_none()
    {
        return None;
    }
    Some(callback_table)
}

fn start_runtime(callbacks: KodosiCallbacks, userdata: *mut c_void) -> *mut c_void {
    if !try_open_lifecycle_gate() {
        tracing::warn!("FFI: kodosi_start_v2 called while a runtime is live or stopping");
        return std::ptr::null_mut();
    }

    let handle = std::panic::catch_unwind(AssertUnwindSafe(|| -> *mut c_void {
        let tokio_rt = match TokioRuntime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("FFI: failed to create tokio runtime: {e}");
                return std::ptr::null_mut();
            }
        };

        let embedded = match tokio_rt.block_on(kodosi_runtime::start_embedded_runtime()) {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("FFI: failed to start embedded runtime: {e}");
                return std::ptr::null_mut();
            }
        };

        let command_sink = embedded.command_sink();
        let (terminal_connect_result_tx, terminal_connect_result_rx) =
            tokio::sync::mpsc::unbounded_channel();
        let callback_admission = Arc::new(CallbackAdmission::open());
        let terminal_connect_result_join = tokio_rt.spawn(drain_terminal_connect_result_lane(
            terminal_connect_result_rx,
            callbacks,
            Arc::clone(&callback_admission),
            SendPtr(userdata),
        ));

        let runtime = Arc::new(FfiRuntime {
            tokio_rt,
            embedded: Mutex::new(Some(embedded)),
            command_sink: StdMutex::new(Some(command_sink)),
            callbacks,
            userdata: SendPtr(userdata),
            callback_admission,
            terminal_connections: TerminalConnections::new(terminal_connect_result_tx),
            terminal_connect_result_join: StdMutex::new(Some(terminal_connect_result_join)),
            spawned: StdMutex::new(Some(JoinSet::new())),
        });

        spawn_event_drains(&runtime);

        let handle = Box::new(FfiHandle {
            runtime: StdMutex::new(Some(runtime)),
        });
        Box::into_raw(handle).cast::<c_void>()
    }))
    .unwrap_or_else(|_| {
        tracing::error!("FFI: panic in kodosi_start_v2");
        std::ptr::null_mut()
    });

    if handle.is_null() {
        RUNTIME_STATE.store(RUNTIME_STOPPED_STATE, Ordering::Release);
    }
    handle
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_stop(handle: *mut c_void) {
    if handle.is_null() {
        tracing::debug!("FFI: kodosi_stop called with null handle");
        return;
    }

    let Some(runtime) = take_runtime_from_handle(handle) else {
        tracing::debug!("FFI: kodosi_stop called without a live runtime");
        return;
    };

    if RUNTIME_STATE
        .compare_exchange(
            RUNTIME_RUNNING_STATE,
            RUNTIME_STOPPING_STATE,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        restore_runtime_to_handle(handle, runtime);
        tracing::debug!("FFI: kodosi_stop lifecycle gate was not running");
        return;
    }

    runtime.callback_admission.close();
    if CallbackScope::is_active() {
        let deferred_runtime = Arc::clone(&runtime);
        if let Err(error) = std::thread::Builder::new()
            .name("kodosi-ffi-stop".to_owned())
            .spawn(move || teardown_runtime(deferred_runtime))
        {
            tracing::error!(%error, "FFI: failed to defer reentrant stop; runtime remains stopping");
            std::mem::forget(runtime);
        }
        return;
    }

    teardown_runtime(runtime);
}

fn teardown_runtime(runtime: Arc<FfiRuntime>) {
    let deadline = Instant::now() + FFI_STOP_GRACE;
    let completed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let pending_terminal_connects = runtime.terminal_connections.close_and_cancel_all();
        for completion in pending_terminal_connects {
            dispatch_terminal_connect_completion(completion);
        }

        drop(lock_poison_ok(&runtime.command_sink).take());

        let embedded = runtime
            .tokio_rt
            .block_on(async { runtime.embedded.lock().await.take() });
        if let Some(embedded) = embedded {
            embedded.shutdown();
            let join_deadline = tokio::time::Instant::from_std(deadline);
            if runtime
                .tokio_rt
                .block_on(embedded.join_before(join_deadline))
                .is_err()
            {
                tracing::warn!("FFI: embedded runtime exceeded or failed its stop grace");
            }
        }

        runtime.tokio_rt.block_on(async {
            let taken = lock_poison_ok(&runtime.spawned).take();
            if let Some(mut spawned) = taken {
                spawned.abort_all();
                while Instant::now() < deadline {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    match tokio::time::timeout(remaining, spawned.join_next()).await {
                        Ok(Some(Err(error))) if error.is_panic() => {
                            tracing::warn!(?error, "FFI: spawned task panicked");
                        }
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
            }
        });

        let terminal_connect_result_join =
            lock_poison_ok(&runtime.terminal_connect_result_join).take();
        if let Some(join) = terminal_connect_result_join {
            let abort = join.abort_handle();
            let remaining = deadline.saturating_duration_since(Instant::now());
            let joined = runtime
                .tokio_rt
                .block_on(async { tokio::time::timeout(remaining, join).await });
            if joined.is_err() {
                abort.abort();
            }
        }

        runtime.callback_admission.wait_until_idle(deadline)
    }));

    if matches!(completed, Ok(true)) {
        drop(runtime);
        RUNTIME_STATE.store(RUNTIME_STOPPED_STATE, Ordering::Release);
        tracing::info!("FFI: runtime stopped");
    } else {
        tracing::error!("FFI: stop grace expired; leaking pinned runtime in STOPPING state");
        std::mem::forget(runtime);
    }
}

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

fn catch_ffi_panic<F: FnOnce() -> i32>(lane: &'static str, f: F) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        tracing::error!(%lane, "FFI: panic caught at boundary");
        KODOSI_FFI_PANIC
    })
}

fn send_error_retcode(error: kodosi_runtime::RuntimeCommandSendError, lane: &str) -> i32 {
    match error {
        kodosi_runtime::RuntimeCommandSendError::Busy => {
            tracing::debug!(%lane, "FFI: lane busy");
            KODOSI_FFI_BUSY
        }
        kodosi_runtime::RuntimeCommandSendError::Stopped => {
            tracing::debug!(%lane, "FFI: rejecting send — runtime stopped");
            KODOSI_FFI_RUNTIME_STOPPED
        }
        kodosi_runtime::RuntimeCommandSendError::SessionNotFound => {
            tracing::debug!(%lane, "FFI: terminal session not found");
            KODOSI_FFI_SESSION_NOT_FOUND
        }
    }
}

macro_rules! send_command {
    ($handle:expr, $json:expr, $len:expr, $cmd_type:ty, $try_method:ident) => {{
        if $handle.is_null() || $json.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        if $len > KODOSI_MAX_FRAME_BYTES {
            tracing::warn!(
                len = $len,
                "FFI: rejected payload larger than KODOSI_MAX_FRAME_BYTES"
            );
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }
        let Some(runtime) = runtime_from_handle($handle) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let slice = unsafe { std::slice::from_raw_parts($json, $len) };
        let command: $cmd_type = match serde_json::from_slice(slice) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("FFI: failed to deserialize command: {e}");
                return KODOSI_FFI_DESER_FAILED;
            }
        };

        match runtime.with_command_sink(|sink| sink.$try_method(command)) {
            Some(Ok(())) => KODOSI_FFI_OK,
            Some(Err(error)) => send_error_retcode(error, stringify!($try_method)),
            None => KODOSI_FFI_RUNTIME_STOPPED,
        }
    }};
}

unsafe fn send_system_command(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    if h.is_null() || json.is_null() {
        return KODOSI_FFI_NULL_HANDLE;
    }
    if len > KODOSI_MAX_FRAME_BYTES {
        return KODOSI_FFI_PAYLOAD_TOO_LARGE;
    }
    let Some(runtime) = runtime_from_handle(h) else {
        return KODOSI_FFI_RUNTIME_STOPPED;
    };

    let slice = unsafe { std::slice::from_raw_parts(json, len) };
    let command: SystemCommand = match serde_json::from_slice(slice) {
        Ok(command) => command,
        Err(error) => {
            tracing::warn!("FFI: failed to deserialize system command: {error}");
            return KODOSI_FFI_DESER_FAILED;
        }
    };
    let result = runtime.with_command_sink(|sink| {
        if kodosi_runtime::system_command_is_account_scoped(&command) {
            sink.try_send_system_scoped(command)
        } else {
            sink.try_send_system(command)
        }
    });
    match result {
        Some(Ok(())) => KODOSI_FFI_OK,
        Some(Err(error)) => send_error_retcode(error, "try_send_system"),
        None => KODOSI_FFI_RUNTIME_STOPPED,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_terminal(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_terminal", || {
        if h.is_null() || json.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        if len > KODOSI_MAX_FRAME_BYTES {
            tracing::warn!(len, "FFI: rejected terminal payload above size limit");
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }
        let Some(runtime) = runtime_from_handle(h) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let slice = unsafe { std::slice::from_raw_parts(json, len) };
        let command: TerminalCommand = match serde_json::from_slice(slice) {
            Ok(command) => command,
            Err(error) => {
                tracing::warn!("FFI: failed to deserialize terminal command: {error}");
                return KODOSI_FFI_DESER_FAILED;
            }
        };
        if matches!(command, TerminalCommand::InputBytes { .. }) {
            tracing::warn!("FFI: terminal input requires the correlated binary input API");
            return KODOSI_FFI_DESER_FAILED;
        }
        let correlation = match &command {
            TerminalCommand::Resize {
                session_id,
                identity,
                ..
            } => Some((
                session_id.clone(),
                TerminalSubscription {
                    id: identity.subscription_id.clone(),
                    generation: identity.subscription_generation,
                },
            )),
            TerminalCommand::Focus { .. } | TerminalCommand::Blur { .. } => None,
            TerminalCommand::InputBytes { .. } => {
                unreachable!("input variants are rejected above")
            }
            TerminalCommand::HeadlessResize { .. } => return KODOSI_FFI_DESER_FAILED,
        };
        let send = || runtime.with_command_sink(|sink| sink.try_send_terminal(command));
        let result = if let Some((session_id, subscription)) = correlation {
            runtime
                .terminal_connections
                .with_current_subscription(&session_id, &subscription, send)
                .ok_or(KODOSI_FFI_STALE_SUBSCRIPTION)
        } else {
            Ok(send())
        };
        match result {
            Ok(Some(Ok(()))) => KODOSI_FFI_OK,
            Ok(Some(Err(error))) => send_error_retcode(error, "try_send_terminal"),
            Ok(None) => KODOSI_FFI_RUNTIME_STOPPED,
            Err(code) => code,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_system(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_system", || unsafe {
        send_system_command(h, json, len)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_auth(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_auth", || {
        send_command!(h, json, len, AuthCommand, try_send_auth)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_friends(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_friends", || {
        send_command!(h, json, len, FriendsCommand, try_send_friends_scoped)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_devices(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_devices", || {
        send_command!(h, json, len, DeviceCommand, try_send_devices_scoped)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_trust(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_trust", || {
        send_command!(h, json, len, TrustCommand, try_send_trust_scoped)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_room(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_room", || {
        send_command!(h, json, len, RoomCommand, try_send_room_scoped)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_sessions(h: *mut c_void, json: *const u8, len: usize) -> i32 {
    catch_ffi_panic("send_sessions", || {
        send_command!(h, json, len, SessionCommand, try_send_session_scoped)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_agent_intel(
    h: *mut c_void,
    json: *const u8,
    len: usize,
) -> i32 {
    catch_ffi_panic("send_agent_intel", || {
        if h.is_null() || json.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        if len > KODOSI_MAX_FRAME_BYTES {
            tracing::warn!(
                len,
                "FFI: rejected payload larger than KODOSI_MAX_FRAME_BYTES"
            );
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }
        let Some(runtime) = runtime_from_handle(h) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let slice = unsafe { std::slice::from_raw_parts(json, len) };
        let command: AgentIntelCommand = match serde_json::from_slice(slice) {
            Ok(c) => c,
            Err(error) => {
                tracing::warn!("FFI: failed to deserialize agent-intel command: {error}");
                return KODOSI_FFI_DESER_FAILED;
            }
        };
        match runtime.with_command_sink(|sink| sink.try_send_agent_intel_scoped(command)) {
            Some(Ok(())) => KODOSI_FFI_OK,
            Some(Err(error)) => send_error_retcode(error, "try_send_agent_intel"),
            None => KODOSI_FFI_RUNTIME_STOPPED,
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_protocol_version() -> u32 {
    kodosi_runtime::protocol_authority::PROTOCOL_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_terminal_semantic_checkpoint_v2_capability() -> u32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_terminal_connect_result_v2_capability() -> u32 {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_input(
    h: *mut c_void,
    session_id: *const c_char,
    expected_runtime_incarnation_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
    bytes: *const u8,
    len: usize,
) -> i32 {
    catch_ffi_panic("terminal_input", || {
        if h.is_null()
            || session_id.is_null()
            || expected_runtime_incarnation_id.is_null()
            || subscription_id.is_null()
        {
            return KODOSI_FFI_NULL_HANDLE;
        }
        if len != 0 && bytes.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        let Some(runtime) = runtime_from_handle(h) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let Ok(sid) = unsafe { CStr::from_ptr(session_id) }.to_str() else {
            return KODOSI_FFI_DESER_FAILED;
        };

        let Ok(expected_incarnation) =
            unsafe { CStr::from_ptr(expected_runtime_incarnation_id) }.to_str()
        else {
            return KODOSI_FFI_DESER_FAILED;
        };
        if expected_incarnation.is_empty() || expected_incarnation.len() > 64 {
            return KODOSI_FFI_DESER_FAILED;
        }

        let subscription = match unsafe {
            parse_terminal_subscription(subscription_id, subscription_generation)
        } {
            Ok(subscription) => subscription,
            Err(code) => return code,
        };
        if len == 0 {
            return runtime
                .terminal_connections
                .with_current_subscription(sid, &subscription, || KODOSI_FFI_OK)
                .unwrap_or(KODOSI_FFI_STALE_SUBSCRIPTION);
        }
        if len > KODOSI_MAX_FRAME_BYTES {
            tracing::warn!(
                len,
                "FFI: rejected terminal input larger than KODOSI_MAX_FRAME_BYTES"
            );
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }

        let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
        let command = TerminalCommand::InputBytes {
            session_id: sid.to_owned(),
            bytes: slice.to_vec(),
            expected_runtime_incarnation_id: expected_incarnation.to_owned(),
            subscription_id: Some(subscription.id.clone()),
            subscription_generation: Some(subscription.generation),
        };
        let send = || runtime.with_command_sink(|sink| sink.try_send_terminal(command));
        let Some(result) =
            runtime
                .terminal_connections
                .with_current_subscription(sid, &subscription, send)
        else {
            return KODOSI_FFI_STALE_SUBSCRIPTION;
        };
        match result {
            Some(Ok(())) => KODOSI_FFI_OK,
            Some(Err(error)) => send_error_retcode(error, "terminal_input"),
            None => KODOSI_FFI_RUNTIME_STOPPED,
        }
    })
}

unsafe fn parse_terminal_subscription(
    subscription_id: *const c_char,
    generation: u64,
) -> Result<TerminalSubscription, i32> {
    if subscription_id.is_null() {
        return Err(KODOSI_FFI_NULL_HANDLE);
    }
    if generation == 0 {
        return Err(KODOSI_FFI_DESER_FAILED);
    }

    let subscription_id = unsafe { CStr::from_ptr(subscription_id) }
        .to_str()
        .map_err(|_| KODOSI_FFI_DESER_FAILED)?;
    if subscription_id.is_empty() || subscription_id.len() > 128 {
        return Err(KODOSI_FFI_DESER_FAILED);
    }
    Ok(TerminalSubscription {
        id: subscription_id.to_owned(),
        generation,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_connect_v2(
    h: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
) -> i32 {
    terminal_connect_impl(h, session_id, subscription_id, subscription_generation)
}

fn terminal_connect_impl(
    h: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(|| -> i32 {
        if h.is_null() || session_id.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        let Some(runtime) = runtime_from_handle(h) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let Ok(sid_str) = unsafe { CStr::from_ptr(session_id) }.to_str() else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Ok(sid) = kodosi_domain::ids::SessionId::try_from(sid_str) else {
            return KODOSI_FFI_DESER_FAILED;
        };

        let requested_subscription = match unsafe {
            parse_terminal_subscription(subscription_id, subscription_generation)
        } {
            Ok(subscription) => subscription,
            Err(code) => return code,
        };

        let Some(sink) = runtime.command_sink_clone() else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };
        if !sink.is_runtime_loop_open() {
            return KODOSI_FFI_RUNTIME_STOPPED;
        }
        let Some((generation, subscription, superseded_completion)) = runtime
            .terminal_connections
            .begin_connect(sid_str, requested_subscription)
        else {
            return if runtime.terminal_connections.registration_is_open() {
                KODOSI_FFI_STALE_SUBSCRIPTION
            } else {
                KODOSI_FFI_RUNTIME_STOPPED
            };
        };
        dispatch_superseded_terminal_connect_result(superseded_completion);
        let sid_owned = sid_str.to_owned();
        let spawned_ok = try_spawn_tracked(
            &runtime,
            finish_terminal_connect(
                sink,
                sid,
                sid_owned,
                generation,
                subscription,
                Arc::clone(&runtime),
            ),
        );
        if !spawned_ok {
            let result = terminal_connect_spawn_failure_result(
                &runtime.terminal_connections,
                sid_str,
                generation,
            );
            tracing::debug!("FFI: terminal_connect skipped — runtime stopping");
            return result;
        }
        KODOSI_FFI_OK
    }))
    .unwrap_or_else(|_| {
        tracing::error!("FFI: panic in kodosi_terminal_connect_v2");
        KODOSI_FFI_PANIC
    })
}

fn terminal_connect_spawn_failure_result(
    connections: &TerminalConnections,
    session_id: &str,
    generation: u64,
) -> i32 {
    if connections.abandon_unspawned_connect(session_id, generation) {
        KODOSI_FFI_OK
    } else {
        KODOSI_FFI_RUNTIME_STOPPED
    }
}

async fn consume_subscriber_bootstrap(
    subscriber: &mut kodosi_runtime::terminal_transport::hub::SubscriberHandle,
    callbacks: &KodosiCallbacks,
    context: &SendCallbackCtx,
) -> Result<u64, i32> {
    let control = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        subscriber.control_rx.recv(),
    )
    .await
    .map_err(|_| KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED)?
    .ok_or(KODOSI_FFI_SESSION_NOT_FOUND)?;
    match control {
        kodosi_runtime::terminal_transport::TerminalControlFrame::SemanticCheckpoint {
            checkpoint,
            next_sequence,
        } => {
            let result = emit_local_checkpoint(callbacks, context, &checkpoint, next_sequence);
            if result == KODOSI_FFI_OK {
                Ok(next_sequence)
            } else {
                Err(result)
            }
        }
        _ => Err(KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED),
    }
}

async fn finish_terminal_connect(
    sink: RuntimeCommandSink,
    sid: kodosi_domain::ids::SessionId,
    sid_owned: String,
    generation: u64,
    subscription: TerminalSubscription,
    runtime: Arc<FfiRuntime>,
) {
    let result = sink
        .subscribe_terminal_hub(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .await;

    match result {
        Ok(mut subscriber) => {
            let cancel_token = tokio_util::sync::CancellationToken::new();
            let refresh = Arc::new(tokio::sync::Notify::new());
            if !runtime.terminal_connections.install(
                &sid_owned,
                generation,
                cancel_token.clone(),
                Arc::clone(&refresh),
            ) {
                tracing::debug!(
                    "FFI: discarding stale terminal subscribe completion for {sid_owned}"
                );
                cancel_token.cancel();
                unregister_terminal_subscriber(&runtime, Some(sid), subscriber.connection_id).await;
                return;
            }
            let connection_id = subscriber.connection_id;
            let context = if let (Ok(c_sid), Ok(c_subscription_id)) = (
                std::ffi::CString::new(sid_owned.as_str()),
                std::ffi::CString::new(subscription.id.as_str()),
            ) {
                SendCallbackCtx {
                    ud: runtime.userdata,
                    admission: Arc::clone(&runtime.callback_admission),
                    c_sid,
                    c_subscription_id,
                    subscription_generation: subscription.generation,
                }
            } else {
                unregister_terminal_subscriber(&runtime, Some(sid), connection_id).await;
                return;
            };
            let next_sequence =
                match consume_subscriber_bootstrap(&mut subscriber, &runtime.callbacks, &context)
                    .await
                {
                    Ok(next_sequence) => next_sequence,
                    Err(result) => {
                        unregister_terminal_subscriber(&runtime, Some(sid), connection_id).await;
                        if let Some(failed) = runtime
                            .terminal_connections
                            .fail_connect_if_current(&sid_owned, generation, result)
                        {
                            dispatch_terminal_connect_completion(failed);
                        }
                        return;
                    }
                };
            if let Some(start_drain) = spawn_terminal_subscriber_drain_from_task(
                &sid_owned,
                subscriber,
                cancel_token,
                refresh,
                generation,
                context,
                next_sequence,
                &runtime,
            ) {
                if let Some(completed) = runtime
                    .terminal_connections
                    .finish_connect_if_current(&sid_owned, generation)
                {
                    dispatch_successful_terminal_connect_completion(completed).await;
                    let _ = start_drain.send(());
                }
            } else {
                unregister_terminal_subscriber(&runtime, Some(sid), connection_id).await;
                if let Some(failed) = runtime.terminal_connections.fail_connect_if_current(
                    &sid_owned,
                    generation,
                    KODOSI_FFI_RUNTIME_STOPPED,
                ) {
                    dispatch_terminal_connect_completion(failed);
                }
            }
        }
        Err(error) => {
            let result = send_error_retcode(error, "terminal_connect");
            if let Some(failed) = runtime
                .terminal_connections
                .fail_connect_if_current(&sid_owned, generation, result)
            {
                dispatch_terminal_connect_completion(failed);
            }
            tracing::warn!("FFI: terminal connect failed for {sid_owned}: {result}");
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_refresh_v2(
    h: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        if h.is_null() || session_id.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        let Some(runtime) = runtime_from_handle(h) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let Ok(sid_str) = unsafe { CStr::from_ptr(session_id) }.to_str() else {
            return KODOSI_FFI_DESER_FAILED;
        };

        let subscription = match unsafe {
            parse_terminal_subscription(subscription_id, subscription_generation)
        } {
            Ok(subscription) => subscription,
            Err(code) => return code,
        };

        if runtime
            .terminal_connections
            .request_refresh(sid_str, &subscription)
        {
            KODOSI_FFI_OK
        } else {
            KODOSI_FFI_SESSION_NOT_FOUND
        }
    }))
    .unwrap_or_else(|_| {
        tracing::error!("FFI: panic in terminal refresh");
        KODOSI_FFI_PANIC
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_terminal_disconnect_v2(
    h: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
) -> i32 {
    terminal_disconnect_impl(h, session_id, subscription_id, subscription_generation)
}

fn terminal_disconnect_impl(
    h: *mut c_void,
    session_id: *const c_char,
    subscription_id: *const c_char,
    subscription_generation: u64,
) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        if h.is_null() || session_id.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        let Some(runtime) = runtime_from_handle(h) else {
            return KODOSI_FFI_RUNTIME_STOPPED;
        };

        let Ok(sid_str) = unsafe { CStr::from_ptr(session_id) }.to_str() else {
            return KODOSI_FFI_DESER_FAILED;
        };

        let requested_subscription = match unsafe {
            parse_terminal_subscription(subscription_id, subscription_generation)
        } {
            Ok(subscription) => subscription,
            Err(code) => return code,
        };

        let (disconnected, pending_completion) = runtime
            .terminal_connections
            .disconnect(sid_str, &requested_subscription);
        if disconnected {
            if let Some(pending) = pending_completion {
                dispatch_terminal_connect_completion(pending);
            }
            tracing::info!("FFI: terminal disconnected for {sid_str}");
            KODOSI_FFI_OK
        } else {
            KODOSI_FFI_SESSION_NOT_FOUND
        }
    }))
    .unwrap_or_else(|_| {
        tracing::error!("FFI: panic in terminal disconnect");
        KODOSI_FFI_PANIC
    })
}

fn spawn_event_drains(runtime: &Arc<FfiRuntime>) {
    let rt = Arc::clone(runtime);

    let _ = try_spawn_tracked(runtime, async move {
        let receivers = {
            let mut guard = rt.embedded.lock().await;
            guard.as_mut().and_then(EmbeddedRuntime::take_events)
        };

        let Some(rx) = receivers else {
            tracing::warn!("FFI: no event receivers available");
            return;
        };

        spawn_per_lane_drains(&rt, rx);
    });
}

fn spawn_per_lane_drains(rt: &Arc<FfiRuntime>, rx: RuntimeEventReceivers) {
    macro_rules! spawn_lane {
        ($rx_field:expr, $cb:expr, $label:literal) => {{
            let lane_runtime = Arc::clone(rt);
            let mut lane_rx = $rx_field;
            let cb = $cb;
            let _ = try_spawn_tracked(rt, async move {
                drain_event_lane(&lane_runtime, &mut lane_rx, cb, $label).await;
            });
        }};
    }

    spawn_lane!(rx.auth_rx, rt.callbacks.on_auth_event, "auth");
    spawn_lane!(rx.sessions_rx, rt.callbacks.on_session_event, "sessions");
    spawn_lane!(rx.system_rx, rt.callbacks.on_system_event, "system");
    spawn_lane!(rx.friends_rx, rt.callbacks.on_friends_event, "friends");
    spawn_lane!(rx.devices_rx, rt.callbacks.on_devices_event, "devices");
    spawn_lane!(rx.trust_rx, rt.callbacks.on_trust_event, "trust");
    spawn_lane!(rx.room_rx, rt.callbacks.on_room_event, "room");
    spawn_lane!(
        rx.agent_global_rx,
        rt.callbacks.on_agent_global_event,
        "agent_global"
    );

    {
        let lane_runtime = Arc::clone(rt);
        let mut lane_rx = rx.agent_intel_rx;
        let cb = rt.callbacks.on_agent_intel_event;
        let _ = try_spawn_tracked(rt, async move {
            drain_agent_intel_lane(&lane_runtime, &mut lane_rx, cb).await;
        });
    }

    {
        let lane_runtime = Arc::clone(rt);
        let mut lane_rx = rx.terminal_control_rx;
        let _ = try_spawn_tracked(rt, async move {
            drain_terminal_control_lane(&lane_runtime, &mut lane_rx).await;
        });
    }
}

async fn drain_event_lane<T: serde::Serialize>(
    rt: &Arc<FfiRuntime>,
    rx: &mut tokio::sync::mpsc::Receiver<T>,
    cb: EventCb,
    lane: &'static str,
) {
    let ud = rt.userdata;
    while let Some(event) = rx.recv().await {
        let Some(callback) = cb else { continue };
        match serde_json::to_vec(&event) {
            Ok(json) => {
                let Some(_permit) = rt.callback_admission.enter() else {
                    break;
                };

                unsafe {
                    callback(json.as_ptr(), json.len(), ud.0);
                }
            }
            Err(error) => {
                tracing::error!(%lane, %error, "FFI: event serialization failed; event dropped");
            }
        }
    }
}

async fn drain_agent_intel_lane(
    rt: &Arc<FfiRuntime>,
    rx: &mut tokio::sync::mpsc::Receiver<kodosi_runtime::AccountAgentIntelEvent>,
    cb: EventCb,
) {
    let ud = rt.userdata;
    while let Some(event) = rx.recv().await {
        let Some(callback) = cb else { continue };
        match serde_json::to_vec(&event) {
            Ok(json) => {
                let Some(_permit) = rt.callback_admission.enter() else {
                    break;
                };

                unsafe {
                    callback(json.as_ptr(), json.len(), ud.0);
                }
            }
            Err(error) => {
                tracing::error!(%error, "FFI: agent-intel event serialization failed");
                let kodosi_runtime::AgentIntelEvent::Reply {
                    request_id,
                    payload,
                } = &event.event
                else {
                    continue;
                };
                let mutation_id = payload
                    .get("mutationId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                let reconciliation_required = mutation_id.is_some();
                let fallback = kodosi_runtime::AccountAgentIntelEvent::new(
                    event.account_user_id.clone(),
                    event.account_epoch,
                    kodosi_runtime::AgentIntelEvent::Error {
                        request_id: request_id.clone(),
                        message: format!("reply serialization failed: {error}"),
                        failure_kind: if reconciliation_required {
                            kodosi_runtime::AgentIntelFailureKind::DeliveryAmbiguous
                        } else {
                            kodosi_runtime::AgentIntelFailureKind::Deterministic
                        },
                        mutation_id,
                        reconciliation_required,
                    },
                );
                if let Ok(json) = serde_json::to_vec(&fallback)
                    && let Some(_permit) = rt.callback_admission.enter()
                {
                    unsafe {
                        callback(json.as_ptr(), json.len(), ud.0);
                    }
                }
            }
        }
    }
}

async fn drain_terminal_control_lane(
    rt: &Arc<FfiRuntime>,
    rx: &mut tokio::sync::mpsc::Receiver<kodosi_runtime::TerminalEvent>,
) {
    let ud = rt.userdata;
    while let Some(event) = rx.recv().await {
        if rt.callbacks.on_terminal_control_v2.is_none() {
            continue;
        }
        let session_id = event.session_id();
        let Ok(json) = serde_json::to_vec(&event) else {
            continue;
        };
        emit_terminal_notification(
            &rt.callbacks,
            &rt.callback_admission,
            &rt.terminal_connections,
            ud,
            session_id,
            &json,
        );
    }
}

fn emit_terminal_notification(
    callbacks: &KodosiCallbacks,
    callback_admission: &CallbackAdmission,
    connections: &TerminalConnections,
    userdata: SendPtr,
    session_id: &str,
    json: &[u8],
) {
    let Ok(c_sid) = std::ffi::CString::new(session_id) else {
        return;
    };

    let Some(subscription) = connections.current_subscription(session_id) else {
        return;
    };
    let (Some(callback), Ok(c_subscription_id)) = (
        callbacks.on_terminal_control_v2,
        std::ffi::CString::new(subscription.id),
    ) else {
        return;
    };
    let Some(_permit) = callback_admission.enter() else {
        return;
    };

    unsafe {
        callback(
            c_sid.as_ptr(),
            c_subscription_id.as_ptr(),
            subscription.generation,
            json.as_ptr(),
            json.len(),
            userdata.raw(),
        );
    }
}

async fn drain_terminal_connect_result_lane(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TerminalConnectResult>,
    callbacks: KodosiCallbacks,
    callback_admission: Arc<CallbackAdmission>,
    userdata: SendPtr,
) {
    while let Some(mut completion) = rx.recv().await {
        invoke_terminal_connect_result_callback(
            &callbacks,
            &callback_admission,
            userdata,
            &completion.session_id,
            &completion.subscription,
            completion.result,
        );
        if let Some(delivered) = completion.delivered.take() {
            let _ = delivered.send(());
        }
    }
}

async fn dispatch_successful_terminal_connect_completion(
    mut completion: TerminalConnectCompletion,
) {
    let (delivered_tx, delivered_rx) = tokio::sync::oneshot::channel();
    completion.event.delivered = Some(delivered_tx);
    dispatch_terminal_connect_completion(completion);
    let _ = delivered_rx.await;
}

fn dispatch_terminal_connect_completion(completion: TerminalConnectCompletion) {
    let TerminalConnectCompletion { tx, event } = completion;
    if let Err(error) = tx.send(event) {
        let event = error.0;
        tracing::debug!(
            session_id = %event.session_id,
            generation = event.subscription.generation,
            result = event.result,
            "FFI: terminal connect callback lane receiver is closed"
        );
    }
}

fn dispatch_superseded_terminal_connect_result(superseded: Option<TerminalConnectCompletion>) {
    if let Some(superseded) = superseded {
        dispatch_terminal_connect_completion(superseded);
    }
}

fn invoke_terminal_connect_result_callback(
    callbacks: &KodosiCallbacks,
    callback_admission: &CallbackAdmission,
    userdata: SendPtr,
    session_id: &str,
    subscription: &TerminalSubscription,
    result: i32,
) {
    let (Some(callback), Ok(c_session_id), Ok(c_subscription_id)) = (
        callbacks.on_terminal_connect_result_v2,
        std::ffi::CString::new(session_id),
        std::ffi::CString::new(subscription.id.as_str()),
    ) else {
        return;
    };
    let _permit = callback_admission.enter_reserved();

    unsafe {
        callback(
            c_session_id.as_ptr(),
            c_subscription_id.as_ptr(),
            subscription.generation,
            result,
            userdata.raw(),
        );
    }
}

fn emit_local_checkpoint(
    callbacks: &KodosiCallbacks,
    context: &SendCallbackCtx,
    checkpoint: &kodosi_domain::terminal::TerminalCheckpointV2,
    next_sequence: u64,
) -> i32 {
    let Some(callback) = callbacks.on_terminal_semantic_checkpoint_v2 else {
        return KODOSI_FFI_REQUIRED_CALLBACK_MISSING;
    };
    if checkpoint.semantic_checkpoint.is_empty()
        || checkpoint.semantic_checkpoint.len() > KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES
    {
        return KODOSI_FFI_PAYLOAD_TOO_LARGE;
    }
    let Some(_permit) = context.admission.enter() else {
        return KODOSI_FFI_RUNTIME_STOPPED;
    };

    unsafe {
        callback(
            context.c_sid.as_ptr(),
            context.c_subscription_id.as_ptr(),
            context.subscription_generation,
            next_sequence,
            checkpoint.rows(),
            checkpoint.cols(),
            checkpoint.semantic_checkpoint.as_ptr(),
            checkpoint.semantic_checkpoint.len(),
            context.ud.raw(),
        )
    }
}

fn terminal_control_wire_json(
    frame: &kodosi_runtime::terminal_transport::TerminalControlFrame,
) -> Option<serde_json::Value> {
    use kodosi_runtime::terminal_transport::TerminalControlFrame;

    match frame {
        TerminalControlFrame::SemanticCheckpoint { .. } => None,
        TerminalControlFrame::Resize {
            rows,
            cols,
            at_sequence,
        } => Some(serde_json::json!({
            "type": "Resize",
            "rows": rows,
            "cols": cols,
            "at_sequence": at_sequence,
        })),
        TerminalControlFrame::Closed {
            reason,
            final_sequence,
        } => Some(serde_json::json!({
            "type": "Closed",
            "reason": format!("{reason}"),
            "finalSequence": final_sequence,
        })),
    }
}

fn emit_subscriber_control(
    callbacks: &KodosiCallbacks,
    context: &SendCallbackCtx,
    frame: &kodosi_runtime::terminal_transport::TerminalControlFrame,
) {
    let Some(callback) = callbacks.on_terminal_control_v2 else {
        return;
    };
    let Some(wire) = terminal_control_wire_json(frame) else {
        tracing::error!("FFI: local checkpoint reached JSON control path");
        return;
    };
    let Ok(json) = serde_json::to_vec(&wire) else {
        return;
    };
    if json.len() > KODOSI_TERMINAL_CONTROL_MAX_BYTES {
        tracing::error!(
            actual = json.len(),
            maximum = KODOSI_TERMINAL_CONTROL_MAX_BYTES,
            "FFI: terminal control frame exceeds the protocol boundary"
        );
        return;
    }
    let Some(_permit) = context.admission.enter() else {
        return;
    };

    unsafe {
        callback(
            context.c_sid.as_ptr(),
            context.c_subscription_id.as_ptr(),
            context.subscription_generation,
            json.as_ptr(),
            json.len(),
            context.ud.raw(),
        );
    }
}

fn emit_subscriber_data(
    callbacks: &KodosiCallbacks,
    context: &SendCallbackCtx,
    frame: &kodosi_runtime::terminal_transport::TerminalDataFrame,
) {
    if let Some(callback) = callbacks.on_terminal_data_v2 {
        let Some(_permit) = context.admission.enter() else {
            return;
        };

        unsafe {
            callback(
                context.c_sid.as_ptr(),
                context.c_subscription_id.as_ptr(),
                context.subscription_generation,
                frame.sequence,
                frame.bytes.as_ptr(),
                frame.bytes.len(),
                context.ud.raw(),
            );
        }
    }
}

const MAX_REFRESH_BUFFERED_FRAMES: usize = 256;

#[derive(Default)]
struct TerminalRefreshGate {
    in_progress: bool,
    buffered_old_data: VecDeque<kodosi_runtime::terminal_transport::TerminalDataFrame>,
}

impl TerminalRefreshGate {
    fn begin(&mut self) {
        self.in_progress = true;
    }

    fn finish_success(&mut self) {
        self.in_progress = false;
        self.buffered_old_data.clear();
    }

    fn take_failed_refresh_data(
        &mut self,
    ) -> VecDeque<kodosi_runtime::terminal_transport::TerminalDataFrame> {
        self.in_progress = false;
        std::mem::take(&mut self.buffered_old_data)
    }
}

type TerminalRefreshFuture = std::pin::Pin<
    Box<
        dyn Future<Output = Option<kodosi_runtime::terminal_transport::hub::SubscriberHandle>>
            + Send,
    >,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingTerminalResize {
    rows: u16,
    cols: u16,
    at_sequence: u64,
}

enum TerminalDrainEvent {
    Cancelled,
    RefreshRequested,
    RefreshCompleted(Option<kodosi_runtime::terminal_transport::hub::SubscriberHandle>),
    Control(Option<kodosi_runtime::terminal_transport::TerminalControlFrame>),
    Data(Option<kodosi_runtime::terminal_transport::TerminalDataFrame>),
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Each boolean is an independent terminal drain protocol gate."
)]
struct TerminalSubscriberDrain {
    sid: String,
    session_id: Option<kodosi_domain::ids::SessionId>,
    connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId,
    data_rx: tokio::sync::mpsc::Receiver<kodosi_runtime::terminal_transport::TerminalDataFrame>,
    control_rx:
        tokio::sync::mpsc::Receiver<kodosi_runtime::terminal_transport::TerminalControlFrame>,
    cancel_token: tokio_util::sync::CancellationToken,
    refresh: Arc<tokio::sync::Notify>,
    generation: u64,
    ctx: SendCallbackCtx,
    runtime: Arc<FfiRuntime>,
    data_open: bool,
    control_open: bool,
    next_data_sequence: Option<u64>,
    pending_resizes: VecDeque<PendingTerminalResize>,
    pending_data: Option<kodosi_runtime::terminal_transport::TerminalDataFrame>,
    pending_close: Option<kodosi_runtime::terminal_transport::TerminalControlFrame>,
    refresh_in_flight: Option<TerminalRefreshFuture>,
    refresh_gate: TerminalRefreshGate,
    shutdown_requested: bool,
    integrity_close_emitted: bool,
}

impl TerminalSubscriberDrain {
    async fn run(mut self, start_rx: tokio::sync::oneshot::Receiver<()>) {
        if start_rx.await.is_err() {
            unregister_terminal_subscriber(&self.runtime, self.session_id, self.connection_id)
                .await;
            self.runtime
                .terminal_connections
                .clear_if_current(&self.sid, self.generation);
            return;
        }
        while self.advance().await {}
        unregister_terminal_subscriber(&self.runtime, self.session_id, self.connection_id).await;
        self.runtime
            .terminal_connections
            .clear_if_current(&self.sid, self.generation);
    }

    async fn advance(&mut self) -> bool {
        if self.pending_data.is_some() && self.pending_close.is_none() && self.control_open {
            match self.control_rx.try_recv() {
                Ok(frame) => {
                    self.handle_control(Some(frame));
                    return true;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.control_open = false;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            }
        }
        if !self.resolve_pending_data() {
            return true;
        }
        if self.finish_close_if_ready() {
            return false;
        }
        if self.shutdown_requested && self.refresh_in_flight.is_none() {
            return false;
        }
        if !self.shutdown_requested
            && self.pending_close.is_none()
            && self.refresh_in_flight.is_none()
            && (!self.data_open || !self.control_open)
        {
            return self.recover_detached_subscriber().await;
        }
        if self.shutdown_requested {
            return false;
        }

        let event = tokio::select! {


            biased;
            () = self.cancel_token.cancelled(), if !self.shutdown_requested => {
                TerminalDrainEvent::Cancelled
            }
            () = self.refresh.notified(),
                if !self.shutdown_requested
                    && self.pending_close.is_none()
                    && self.refresh_in_flight.is_none() =>
            {
                TerminalDrainEvent::RefreshRequested
            }
            result = Self::poll_refresh(&mut self.refresh_in_flight),
                if self.refresh_in_flight.is_some() =>
            {
                TerminalDrainEvent::RefreshCompleted(result)
            }
            frame = self.control_rx.recv(),
                if self.pending_close.is_none() && self.data_open && self.control_open =>
            {
                TerminalDrainEvent::Control(frame)
            }
            frame = self.data_rx.recv(), if self.data_open => {
                TerminalDrainEvent::Data(frame)
            }
        };

        match event {
            TerminalDrainEvent::Cancelled => {
                self.shutdown_requested = true;
                self.data_open = false;
                self.control_open = false;
                true
            }
            TerminalDrainEvent::RefreshRequested => {
                self.begin_refresh();
                true
            }
            TerminalDrainEvent::RefreshCompleted(result) => self.finish_refresh(result).await,
            TerminalDrainEvent::Control(frame) => {
                self.handle_control(frame);
                true
            }
            TerminalDrainEvent::Data(frame) => {
                self.handle_data(frame);
                true
            }
        }
    }

    async fn poll_refresh(
        refresh: &mut Option<TerminalRefreshFuture>,
    ) -> Option<kodosi_runtime::terminal_transport::hub::SubscriberHandle> {
        match refresh.as_mut() {
            Some(refresh) => refresh.await,
            None => std::future::pending().await,
        }
    }

    fn begin_refresh(&mut self) {
        let runtime = Arc::clone(&self.runtime);
        let sid = self.sid.clone();
        self.refresh_in_flight = Some(Box::pin(
            async move { refresh_terminal(&runtime, &sid).await },
        ));
        self.refresh_gate.begin();
    }

    async fn finish_refresh(
        &mut self,
        result: Option<kodosi_runtime::terminal_transport::hub::SubscriberHandle>,
    ) -> bool {
        self.refresh_in_flight = None;
        if self.shutdown_requested {
            self.refresh_gate.finish_success();
            if let Some(handle) = result {
                unregister_terminal_subscriber(
                    &self.runtime,
                    self.session_id,
                    handle.connection_id,
                )
                .await;
            }
            return false;
        }
        if let Some(mut handle) = result {
            let next_sequence = match consume_subscriber_bootstrap(
                &mut handle,
                &self.runtime.callbacks,
                &self.ctx,
            )
            .await
            {
                Ok(next_sequence) => next_sequence,
                Err(result) => {
                    tracing::error!(session = %self.sid, result, "FFI: terminal refresh bootstrap rejected");
                    unregister_terminal_subscriber(
                        &self.runtime,
                        self.session_id,
                        handle.connection_id,
                    )
                    .await;
                    self.fail_closed(format!("terminal refresh checkpoint rejected ({result})"));
                    return false;
                }
            };
            let previous_connection_id = self.connection_id;
            self.install_subscriber(handle, next_sequence);
            self.refresh_gate.finish_success();
            unregister_terminal_subscriber(&self.runtime, self.session_id, previous_connection_id)
                .await;
            tracing::info!("FFI: terminal checkpoint refreshed for {}", self.sid);
        } else {
            let buffered = self.refresh_gate.take_failed_refresh_data();
            for frame in buffered {
                self.handle_data(Some(frame));
                if self.shutdown_requested {
                    return false;
                }
                self.drain_replayed_pending_data();
                if self.shutdown_requested {
                    return false;
                }
            }
            tracing::warn!(session = %self.sid, "FFI: terminal refresh failed; continuing old subscriber without loss");
        }
        true
    }

    async fn recover_detached_subscriber(&mut self) -> bool {
        if let Some(mut handle) =
            resubscribe_terminal(&self.runtime, &self.sid, &self.cancel_token).await
        {
            let next_sequence = match consume_subscriber_bootstrap(
                &mut handle,
                &self.runtime.callbacks,
                &self.ctx,
            )
            .await
            {
                Ok(next_sequence) => next_sequence,
                Err(result) => {
                    tracing::error!(session = %self.sid, result, "FFI: terminal recovery bootstrap rejected");
                    unregister_terminal_subscriber(
                        &self.runtime,
                        self.session_id,
                        handle.connection_id,
                    )
                    .await;
                    self.fail_closed(format!("terminal recovery checkpoint rejected ({result})"));
                    return false;
                }
            };
            self.install_subscriber(handle, next_sequence);
            return true;
        }
        if !self.cancel_token.is_cancelled() {
            emit_subscriber_control(
                &self.runtime.callbacks,
                &self.ctx,
                &kodosi_runtime::terminal_transport::TerminalControlFrame::Closed {
                    reason:
                        kodosi_runtime::terminal_transport::TerminalCloseReason::RelayDisconnected,
                    final_sequence: self.next_data_sequence.unwrap_or(0),
                },
            );
        }
        false
    }

    fn install_subscriber(
        &mut self,
        handle: kodosi_runtime::terminal_transport::hub::SubscriberHandle,
        next_sequence: u64,
    ) {
        self.connection_id = handle.connection_id;
        self.data_rx = handle.data_rx;
        self.control_rx = handle.control_rx;
        self.data_open = true;
        self.control_open = true;
        self.next_data_sequence = Some(next_sequence);
        self.pending_resizes.clear();
        self.pending_data = None;
        self.pending_close = None;
    }

    fn fail_closed(&mut self, message: impl Into<String>) {
        if self.integrity_close_emitted {
            return;
        }
        let message = message.into();
        tracing::error!(
            session_id = %self.sid,
            expected = ?self.next_data_sequence,
            pending_resize = ?self.pending_resizes.front(),
            pending_data = ?self.pending_data.as_ref().map(|frame| frame.sequence),
            %message,
            "terminal subscriber integrity failure"
        );
        emit_subscriber_control(
            &self.runtime.callbacks,
            &self.ctx,
            &kodosi_runtime::terminal_transport::TerminalControlFrame::Closed {
                reason: kodosi_runtime::terminal_transport::TerminalCloseReason::IoError(message),
                final_sequence: self.pending_data_cursor().unwrap_or(0),
            },
        );
        self.integrity_close_emitted = true;
        self.pending_resizes.clear();
        self.pending_data = None;
        self.pending_close = None;
        self.shutdown_requested = true;
        self.data_open = false;
        self.control_open = false;
    }

    fn pending_data_cursor(&self) -> Option<u64> {
        self.pending_data
            .as_ref()
            .map_or(self.next_data_sequence, |frame| Some(frame.sequence))
    }

    fn queue_resize(&mut self, resize: PendingTerminalResize) -> bool {
        let cursor = self.pending_data_cursor();
        let Some(cursor) = cursor else {
            self.fail_closed("terminal resize arrived before checkpoint cursor");
            return false;
        };
        if resize.at_sequence < cursor {
            self.fail_closed("terminal resize boundary regressed behind data cursor");
            return false;
        }
        if let Some(previous) = self.pending_resizes.back()
            && resize.at_sequence < previous.at_sequence
        {
            self.fail_closed("terminal resize boundaries crossed, duplicated, or regressed");
            return false;
        }
        self.pending_resizes.push_back(resize);
        true
    }

    fn drain_replayed_pending_data(&mut self) {
        while self.pending_data.is_some() && !self.shutdown_requested {
            let _ = self.resolve_pending_data();
        }
    }

    fn resolve_pending_data(&mut self) -> bool {
        let Some(frame) = self.pending_data.as_ref() else {
            if let Some(resize) = self.pending_resizes.front().copied()
                && self.next_data_sequence == Some(resize.at_sequence)
                && self.pending_close.is_none()
            {
                let frame = kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
                    rows: resize.rows,
                    cols: resize.cols,
                    at_sequence: resize.at_sequence,
                };
                emit_subscriber_control(&self.runtime.callbacks, &self.ctx, &frame);
                self.pending_resizes.pop_front();
                return false;
            }
            return true;
        };
        let sequence = frame.sequence;
        if let Some(resize) = self.pending_resizes.front().copied() {
            if sequence > resize.at_sequence {
                self.fail_closed("terminal data crossed a pending resize boundary");
                return true;
            }
            if sequence == resize.at_sequence {
                let frame = kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
                    rows: resize.rows,
                    cols: resize.cols,
                    at_sequence: resize.at_sequence,
                };
                emit_subscriber_control(&self.runtime.callbacks, &self.ctx, &frame);
                self.pending_resizes.pop_front();
                return false;
            }
        }
        let Some(frame) = self.pending_data.take() else {
            return true;
        };
        emit_subscriber_data(&self.runtime.callbacks, &self.ctx, &frame);
        true
    }

    fn finish_close_if_ready(&mut self) -> bool {
        let Some(kodosi_runtime::terminal_transport::TerminalControlFrame::Closed {
            final_sequence,
            ..
        }) = self.pending_close.as_ref()
        else {
            return false;
        };
        let cursor = self.pending_data_cursor().unwrap_or(0);
        if let Some(resize) = self.pending_resizes.front()
            && resize.at_sequence >= *final_sequence
        {
            self.fail_closed("terminal closed before pending resize boundary was reachable");
            return false;
        }
        if self.pending_resizes.is_empty()
            && self.pending_data.is_none()
            && cursor > *final_sequence
        {
            self.fail_closed("terminal close boundary regressed behind data cursor");
            return false;
        }
        if self.pending_data.is_none() && self.next_data_sequence.is_none() && *final_sequence != 0
        {
            self.fail_closed(format!(
                "terminal close boundary {final_sequence} arrived before checkpoint cursor"
            ));
            return false;
        }
        if cursor != *final_sequence {
            return false;
        }
        if self.pending_data.is_some() {
            return false;
        }
        let Some(close) = self.pending_close.take() else {
            return false;
        };
        emit_subscriber_control(&self.runtime.callbacks, &self.ctx, &close);
        self.shutdown_requested = true;
        self.data_open = false;
        self.control_open = false;
        true
    }

    fn handle_control(
        &mut self,
        frame: Option<kodosi_runtime::terminal_transport::TerminalControlFrame>,
    ) {
        let Some(frame) = frame else {
            self.control_open = false;
            if !self.pending_resizes.is_empty() {
                self.fail_closed("terminal control channel closed before pending resize boundary");
            }
            return;
        };
        if matches!(
            &frame,
            kodosi_runtime::terminal_transport::TerminalControlFrame::Closed { .. }
        ) {
            self.pending_close = Some(frame);
            self.control_open = false;
            return;
        }
        if let kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
            rows,
            cols,
            at_sequence,
        } = &frame
        {
            self.queue_resize(PendingTerminalResize {
                rows: *rows,
                cols: *cols,
                at_sequence: *at_sequence,
            });
            return;
        }
        if let kodosi_runtime::terminal_transport::TerminalControlFrame::SemanticCheckpoint {
            checkpoint,
            next_sequence,
        } = &frame
        {
            let result = emit_local_checkpoint(
                &self.runtime.callbacks,
                &self.ctx,
                checkpoint,
                *next_sequence,
            );
            if result == KODOSI_FFI_OK {
                self.next_data_sequence = Some(*next_sequence);
                self.pending_resizes.clear();
                self.pending_data = None;
                self.pending_close = None;
            } else {
                tracing::error!(
                    session = %self.sid,
                    result,
                    "replacement semantic checkpoint rejected"
                );
                self.fail_closed(format!(
                    "replacement semantic checkpoint rejected ({result})"
                ));
            }
            return;
        }
        emit_subscriber_control(&self.runtime.callbacks, &self.ctx, &frame);
    }

    fn handle_data(
        &mut self,
        frame: Option<kodosi_runtime::terminal_transport::TerminalDataFrame>,
    ) {
        let Some(frame) = frame else {
            self.data_open = false;
            if !self.pending_resizes.is_empty() {
                self.fail_closed("terminal data channel closed before pending resize boundary");
            } else if self.pending_close.is_some() {
                tracing::error!(
                    session_id = %self.sid,
                    expected = ?self.next_data_sequence,
                    "terminal data channel closed before close boundary"
                );
                self.fail_closed("terminal data channel closed before close boundary");
            }
            return;
        };
        if self.refresh_gate.in_progress {
            if self.refresh_gate.buffered_old_data.len() >= MAX_REFRESH_BUFFERED_FRAMES {
                self.fail_closed("terminal refresh exceeded its bounded frame buffer");
            } else {
                self.refresh_gate.buffered_old_data.push_back(frame);
            }
            return;
        }
        if self.pending_data.is_some() {
            self.fail_closed("terminal subscriber received data while boundary data was pending");
            return;
        }
        match kodosi_runtime::terminal_transport::classify_data_frame(
            frame.sequence,
            &mut self.next_data_sequence,
        ) {
            kodosi_runtime::terminal_transport::TerminalDataSequenceDecision::Stale => {}
            kodosi_runtime::terminal_transport::TerminalDataSequenceDecision::Exact => {
                self.pending_data = Some(frame);
            }
            kodosi_runtime::terminal_transport::TerminalDataSequenceDecision::Gap {
                expected,
                actual,
            } => {
                tracing::error!(
                    session_id = %self.sid,
                    expected,
                    actual,
                    "terminal subscriber sequence gap before close boundary"
                );
                self.fail_closed(format!(
                    "terminal subscriber sequence gap: expected {expected}, got {actual}"
                ));
            }
            kodosi_runtime::terminal_transport::TerminalDataSequenceDecision::Exhausted => {
                tracing::error!(
                    session_id = %self.sid,
                    sequence = frame.sequence,
                    "terminal subscriber sequence exhausted"
                );
                self.fail_closed(format!(
                    "terminal subscriber sequence exhausted at {}",
                    frame.sequence
                ));
            }
        }
    }
}

fn spawn_terminal_subscriber_drain_from_task(
    session_id: &str,
    subscriber: kodosi_runtime::terminal_transport::hub::SubscriberHandle,
    cancel_token: tokio_util::sync::CancellationToken,
    refresh: Arc<tokio::sync::Notify>,
    generation: u64,
    ctx: SendCallbackCtx,
    next_sequence: u64,
    runtime: &Arc<FfiRuntime>,
) -> Option<tokio::sync::oneshot::Sender<()>> {
    let sid = session_id.to_owned();

    let drain = TerminalSubscriberDrain {
        sid: sid.clone(),
        session_id: kodosi_domain::ids::SessionId::try_from(sid.as_str()).ok(),
        connection_id: subscriber.connection_id,
        data_rx: subscriber.data_rx,
        control_rx: subscriber.control_rx,
        cancel_token,
        refresh,
        generation,
        ctx,
        runtime: Arc::clone(runtime),
        data_open: true,
        control_open: true,
        next_data_sequence: Some(next_sequence),
        pending_resizes: VecDeque::new(),
        pending_data: None,
        pending_close: None,
        refresh_in_flight: None,
        refresh_gate: TerminalRefreshGate::default(),
        shutdown_requested: false,
        integrity_close_emitted: false,
    };
    let (start_tx, start_rx) = tokio::sync::oneshot::channel();
    if !try_spawn_tracked(runtime, drain.run(start_rx)) {
        tracing::debug!(session = %sid, "FFI: terminal drain spawn rejected during stop");
        return None;
    }
    Some(start_tx)
}

async fn unregister_terminal_subscriber(
    runtime: &FfiRuntime,
    session_id: Option<kodosi_domain::ids::SessionId>,
    connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId,
) {
    let (Some(sink), Some(session_id)) = (runtime.command_sink_clone(), session_id) else {
        return;
    };
    if let Err(error) = sink
        .unsubscribe_terminal_hub(session_id, connection_id)
        .await
    {
        tracing::debug!(
            %error,
            %session_id,
            connection_id = ?connection_id,
            "FFI: exact terminal hub unsubscribe did not complete",
        );
    }
}

async fn refresh_terminal(
    rt: &Arc<FfiRuntime>,
    sid_str: &str,
) -> Option<kodosi_runtime::terminal_transport::hub::SubscriberHandle> {
    let sid = kodosi_domain::ids::SessionId::try_from(sid_str).ok()?;
    let sink = rt.command_sink_clone()?;
    if !sink.is_runtime_loop_open() {
        return None;
    }
    sink.subscribe_terminal_hub(sid, TerminalSurface::Desktop, TerminalCapability::Write)
        .await
        .ok()
}

async fn resubscribe_terminal(
    rt: &Arc<FfiRuntime>,
    sid_str: &str,
    cancel_token: &tokio_util::sync::CancellationToken,
) -> Option<kodosi_runtime::terminal_transport::hub::SubscriberHandle> {
    let sid = kodosi_domain::ids::SessionId::try_from(sid_str).ok()?;
    let sink = rt.command_sink_clone()?;
    if let Some(handle) = kodosi_runtime::terminal_transport::resubscribe_terminal_hub(
        &sink,
        sid,
        TerminalSurface::Desktop,
        TerminalCapability::Write,
        cancel_token,
    )
    .await
    {
        tracing::warn!("FFI: terminal resubscribed for {sid_str} after hub detach");
        return Some(handle);
    }
    tracing::warn!("FFI: terminal resubscribe gave up for {sid_str} (session ended?)");
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

    use std::collections::VecDeque;
    use std::ffi::{CStr, c_char, c_void};
    use std::ptr;
    use std::sync::{Arc, Mutex, OnceLock};

    use super::{
        CallbackAdmission, FfiHandle, FfiRuntime, KODOSI_FFI_NULL_HANDLE, KODOSI_FFI_OK,
        KODOSI_FFI_PAYLOAD_TOO_LARGE, KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_SESSION_NOT_FOUND,
        KODOSI_FFI_STALE_SUBSCRIPTION, KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED,
        KODOSI_MAX_FRAME_BYTES, KodosiCallbacks, PendingTerminalResize, RUNTIME_RUNNING_STATE,
        RUNTIME_STATE, RUNTIME_STOPPED_STATE, SendCallbackCtx, SendPtr, TerminalCapability,
        TerminalConnections, TerminalRefreshGate, TerminalSubscriberDrain, TerminalSubscription,
        TerminalSurface, consume_subscriber_bootstrap, decode_callbacks_v2,
        dispatch_superseded_terminal_connect_result, dispatch_terminal_connect_completion,
        drain_terminal_connect_result_lane, emit_local_checkpoint, emit_subscriber_control,
        emit_subscriber_data, emit_terminal_notification, invoke_terminal_connect_result_callback,
        kodosi_protocol_version, kodosi_send_system, kodosi_start_v2, kodosi_stop,
        kodosi_terminal_disconnect_v2, kodosi_terminal_input, lock_poison_ok, runtime_from_handle,
        spawn_terminal_subscriber_drain_from_task, terminal_connect_spawn_failure_result,
        try_open_lifecycle_gate,
    };

    fn runtime_state_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    const fn empty_callbacks() -> KodosiCallbacks {
        KodosiCallbacks {
            on_auth_event: None,
            on_session_event: None,
            on_system_event: None,
            on_friends_event: None,
            on_devices_event: None,
            on_agent_intel_event: None,
            on_agent_global_event: None,
            on_trust_event: None,
            on_room_event: None,
            on_terminal_data_v2: None,
            on_terminal_control_v2: None,
            on_terminal_connect_result_v2: None,
            on_terminal_semantic_checkpoint_v2: None,
        }
    }

    #[test]
    fn stop_null_handle_does_not_release_live_singleton_gate() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        RUNTIME_STATE.store(RUNTIME_RUNNING_STATE, std::sync::atomic::Ordering::Release);

        unsafe { kodosi_stop(ptr::null_mut()) };

        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            RUNTIME_RUNNING_STATE
        );
        RUNTIME_STATE.store(RUNTIME_STOPPED_STATE, std::sync::atomic::Ordering::Release);
    }

    #[test]
    fn reentrant_stop_defers_without_nested_blocking_and_reopens_after_callback() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let runtime = runtime_shell();
        let handle = Box::into_raw(Box::new(FfiHandle {
            runtime: std::sync::Mutex::new(Some(Arc::clone(&runtime))),
        }))
        .cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        let callback = runtime
            .callback_admission
            .enter()
            .expect("callback admitted before stop");
        let started = std::time::Instant::now();

        unsafe { kodosi_stop(handle) };
        assert!(started.elapsed() < std::time::Duration::from_millis(250));
        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            super::RUNTIME_STOPPING_STATE
        );
        drop(callback);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire) != RUNTIME_STOPPED_STATE
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            RUNTIME_STOPPED_STATE
        );
    }

    #[test]
    fn protocol_version_matches_runtime_authority() {
        assert_eq!(
            kodosi_protocol_version(),
            kodosi_runtime::protocol_authority::PROTOCOL_VERSION
        );
        assert_eq!(
            super::kodosi_terminal_semantic_checkpoint_v2_capability(),
            1
        );
    }

    #[test]
    fn start_v2_rejects_any_table_size_other_than_abi_v4() {
        let callbacks = empty_callbacks();
        let table_size = std::mem::size_of::<KodosiCallbacks>();

        unsafe {
            assert!(
                decode_callbacks_v2(&raw const callbacks, table_size.saturating_sub(1)).is_none()
            );
            assert!(decode_callbacks_v2(&raw const callbacks, table_size + 1).is_none());
        }
    }

    unsafe extern "C" fn test_terminal_data_v2(
        _session_id: *const c_char,
        _subscription_id: *const c_char,
        _subscription_generation: u64,
        _sequence: u64,
        _bytes: *const u8,
        _len: usize,
        _userdata: *mut c_void,
    ) {
    }

    unsafe extern "C" fn test_terminal_control_v2(
        _session_id: *const c_char,
        _subscription_id: *const c_char,
        _subscription_generation: u64,
        _json: *const u8,
        _len: usize,
        _userdata: *mut c_void,
    ) {
    }

    unsafe extern "C" fn test_terminal_connect_result_v2(
        _session_id: *const c_char,
        _subscription_id: *const c_char,
        _subscription_generation: u64,
        _result: i32,
        _userdata: *mut c_void,
    ) {
    }

    unsafe extern "C" fn test_terminal_semantic_checkpoint_v2(
        _session_id: *const c_char,
        _subscription_id: *const c_char,
        _subscription_generation: u64,
        _next_sequence: u64,
        _rows: u16,
        _cols: u16,
        _semantic_json: *const u8,
        _semantic_json_len: usize,
        _userdata: *mut c_void,
    ) -> i32 {
        KODOSI_FFI_OK
    }

    #[test]
    fn start_v2_requires_the_exact_abi_v4_callback_table() {
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(test_terminal_data_v2);
        callbacks.on_terminal_control_v2 = Some(test_terminal_control_v2);
        callbacks.on_terminal_connect_result_v2 = Some(test_terminal_connect_result_v2);
        callbacks.on_terminal_semantic_checkpoint_v2 = Some(test_terminal_semantic_checkpoint_v2);
        let data_end = std::mem::offset_of!(KodosiCallbacks, on_terminal_data_v2)
            + std::mem::size_of::<super::TerminalDataV2Cb>();
        let control_end = std::mem::offset_of!(KodosiCallbacks, on_terminal_control_v2)
            + std::mem::size_of::<super::TerminalControlV2Cb>();
        let connect_end = std::mem::offset_of!(KodosiCallbacks, on_terminal_connect_result_v2)
            + std::mem::size_of::<super::TerminalConnectResultV2Cb>();
        let checkpoint_end =
            std::mem::offset_of!(KodosiCallbacks, on_terminal_semantic_checkpoint_v2)
                + std::mem::size_of::<super::TerminalSemanticCheckpointV2Cb>();

        unsafe {
            for incomplete in [
                data_end - 1,
                data_end,
                control_end,
                connect_end,
                checkpoint_end - 1,
            ] {
                assert!(
                    decode_callbacks_v2(&raw const callbacks, incomplete).is_none(),
                    "ABI v5 must reject callback table size {incomplete}"
                );
            }

            let complete = decode_callbacks_v2(&raw const callbacks, checkpoint_end)
                .expect("complete ABI v5 callback table");
            assert!(complete.on_terminal_data_v2.is_some());
            assert!(complete.on_terminal_control_v2.is_some());
            assert!(complete.on_terminal_connect_result_v2.is_some());
            assert!(complete.on_terminal_semantic_checkpoint_v2.is_some());
            assert!(decode_callbacks_v2(&raw const callbacks, checkpoint_end + 64).is_none());
        }
    }

    #[test]
    fn stopping_old_tombstone_does_not_release_new_runtime_gate() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let tombstone = Box::into_raw(Box::new(FfiHandle {
            runtime: std::sync::Mutex::new(None),
        }))
        .cast();
        RUNTIME_STATE.store(RUNTIME_RUNNING_STATE, std::sync::atomic::Ordering::Release);

        unsafe { kodosi_stop(tombstone) };

        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            RUNTIME_RUNNING_STATE
        );
        RUNTIME_STATE.store(RUNTIME_STOPPED_STATE, std::sync::atomic::Ordering::Release);
    }

    fn runtime_shell() -> std::sync::Arc<FfiRuntime> {
        runtime_shell_with(empty_callbacks(), SendPtr(ptr::null_mut()))
    }

    fn runtime_shell_with(
        callbacks: KodosiCallbacks,
        userdata: SendPtr,
    ) -> std::sync::Arc<FfiRuntime> {
        let tokio_rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let (terminal_connect_result_tx, terminal_connect_result_rx) =
            tokio::sync::mpsc::unbounded_channel();
        let callback_admission = Arc::new(CallbackAdmission::open());
        let terminal_connect_result_join = tokio_rt.spawn(drain_terminal_connect_result_lane(
            terminal_connect_result_rx,
            callbacks,
            Arc::clone(&callback_admission),
            userdata,
        ));
        std::sync::Arc::new(FfiRuntime {
            tokio_rt,
            embedded: tokio::sync::Mutex::new(None),
            command_sink: std::sync::Mutex::new(None),
            callbacks,
            userdata,
            callback_admission,
            terminal_connections: TerminalConnections::new(terminal_connect_result_tx),
            terminal_connect_result_join: std::sync::Mutex::new(Some(terminal_connect_result_join)),
            spawned: std::sync::Mutex::new(Some(tokio::task::JoinSet::new())),
        })
    }

    #[test]
    fn second_start_while_running_returns_null() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        assert!(try_open_lifecycle_gate(), "gate must start closed-to-open");
        let callbacks = empty_callbacks();

        let handle = unsafe {
            kodosi_start_v2(
                &raw const callbacks,
                std::mem::size_of::<KodosiCallbacks>(),
                ptr::null_mut(),
            )
        };

        assert!(handle.is_null());
        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            RUNTIME_RUNNING_STATE,
            "a rejected start must leave the live runtime's gate untouched"
        );
        RUNTIME_STATE.store(RUNTIME_STOPPED_STATE, std::sync::atomic::Ordering::Release);
    }

    #[test]
    fn stop_reopens_the_gate_and_leaves_a_reusable_tombstone() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime_shell())),
        };

        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            RUNTIME_STOPPED_STATE
        );
        assert!(runtime_from_handle(handle_ptr).is_none());

        unsafe { kodosi_stop(handle_ptr) };
        assert_eq!(
            RUNTIME_STATE.load(std::sync::atomic::Ordering::Acquire),
            RUNTIME_STOPPED_STATE
        );

        assert!(
            try_open_lifecycle_gate(),
            "start after stop must be admitted by the singleton gate"
        );
        RUNTIME_STATE.store(RUNTIME_STOPPED_STATE, std::sync::atomic::Ordering::Release);
    }

    #[test]
    fn null_handle_send_returns_stable_retcode() {
        let payload = br#"{"type":"shutdown"}"#;

        let rc = unsafe { kodosi_send_system(ptr::null_mut(), payload.as_ptr(), payload.len()) };

        assert_eq!(rc, KODOSI_FFI_NULL_HANDLE);
    }

    #[test]
    fn zero_terminal_input_validates_stopped_handle_before_noop() {
        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(None),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        let session_id = c"018f0000-0000-7000-8000-000000000000";
        let incarnation_id = c"01900000-0000-7000-8000-000000000001";
        let subscription_id = c"terminal-1";

        let result = unsafe {
            kodosi_terminal_input(
                handle_ptr,
                session_id.as_ptr(),
                incarnation_id.as_ptr(),
                subscription_id.as_ptr(),
                1,
                ptr::null(),
                0,
            )
        };
        assert_eq!(result, KODOSI_FFI_RUNTIME_STOPPED);
    }

    #[test]
    fn terminal_input_rejects_null_bytes_when_non_empty() {
        let fake_handle = std::ptr::dangling_mut();
        let session_id = c"018f0000-0000-7000-8000-000000000000";
        let incarnation_id = c"01900000-0000-7000-8000-000000000001";
        let subscription_id = c"terminal-1";

        let rc = unsafe {
            kodosi_terminal_input(
                fake_handle,
                session_id.as_ptr(),
                incarnation_id.as_ptr(),
                subscription_id.as_ptr(),
                1,
                ptr::null(),
                1,
            )
        };

        assert_eq!(rc, KODOSI_FFI_NULL_HANDLE);
    }

    #[test]
    fn oversized_payload_rejects_before_touching_handle() {
        let fake_handle = std::ptr::dangling_mut();
        let payload = *b"{";

        let rc = unsafe {
            kodosi_send_system(
                fake_handle,
                payload.as_ptr(),
                KODOSI_MAX_FRAME_BYTES.saturating_add(1),
            )
        };

        assert_eq!(rc, KODOSI_FFI_PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn stale_terminal_lifecycle_work_cannot_cancel_newer_connection() {
        let connections = TerminalConnections::default();
        let first_subscription = TerminalSubscription {
            id: "first".to_owned(),
            generation: 1,
        };
        let (first_generation, _, _) = connections
            .begin_connect("session", first_subscription.clone())
            .unwrap();
        let first = tokio_util::sync::CancellationToken::new();
        assert!(connections.install(
            "session",
            first_generation,
            first.clone(),
            Arc::new(tokio::sync::Notify::new()),
        ));

        assert!(connections.disconnect("session", &first_subscription).0);
        assert!(first.is_cancelled());

        let second_subscription = TerminalSubscription {
            id: "second".to_owned(),
            generation: 2,
        };
        let (second_generation, _, _) = connections
            .begin_connect("session", second_subscription)
            .unwrap();
        let second = tokio_util::sync::CancellationToken::new();
        assert!(connections.install(
            "session",
            second_generation,
            second.clone(),
            Arc::new(tokio::sync::Notify::new()),
        ));

        connections.clear_if_current("session", first_generation);
        assert!(!second.is_cancelled());
        assert!(
            lock_poison_ok(&connections.inner)
                .states
                .get("session")
                .is_some_and(|state| state.generation == second_generation)
        );
    }

    #[test]
    fn subscribe_completion_after_disconnect_is_rejected_by_generation() {
        let connections = TerminalConnections::default();
        let stale_subscription = TerminalSubscription {
            id: "stale".to_owned(),
            generation: 1,
        };
        let (stale_generation, _, _) = connections
            .begin_connect("session", stale_subscription.clone())
            .unwrap();
        assert!(connections.disconnect("session", &stale_subscription).0);
        let current_subscription = TerminalSubscription {
            id: "current".to_owned(),
            generation: 2,
        };
        let (current_generation, _, _) = connections
            .begin_connect("session", current_subscription)
            .unwrap();
        let current = tokio_util::sync::CancellationToken::new();
        assert!(connections.install(
            "session",
            current_generation,
            current.clone(),
            Arc::new(tokio::sync::Notify::new()),
        ));

        assert!(!connections.install(
            "session",
            stale_generation,
            tokio_util::sync::CancellationToken::new(),
            Arc::new(tokio::sync::Notify::new()),
        ));
        assert!(!current.is_cancelled());
    }

    #[test]
    fn stale_subscription_id_disconnect_cannot_cancel_replacement() {
        let connections = TerminalConnections::default();
        let first_subscription = TerminalSubscription {
            id: "first".to_owned(),
            generation: 100,
        };
        let (first_generation, _, _) = connections
            .begin_connect("session", first_subscription.clone())
            .unwrap();
        let first = tokio_util::sync::CancellationToken::new();
        assert!(connections.install(
            "session",
            first_generation,
            first.clone(),
            Arc::new(tokio::sync::Notify::new()),
        ));

        let second_subscription = TerminalSubscription {
            id: "second".to_owned(),
            generation: 200,
        };
        let (second_generation, _, superseded) = connections
            .begin_connect("session", second_subscription.clone())
            .unwrap();
        let superseded = superseded.expect("first completion reserved");
        assert_eq!(superseded.event.subscription, first_subscription);
        assert_eq!(superseded.event.result, KODOSI_FFI_STALE_SUBSCRIPTION);
        let second = tokio_util::sync::CancellationToken::new();
        assert!(connections.install(
            "session",
            second_generation,
            second.clone(),
            Arc::new(tokio::sync::Notify::new()),
        ));
        assert!(first.is_cancelled());

        assert!(!connections.disconnect("session", &first_subscription).0);
        assert!(!second.is_cancelled());
        assert!(connections.disconnect("session", &second_subscription).0);
        assert!(second.is_cancelled());
    }

    #[test]
    fn resize_admission_requires_exact_current_subscription() {
        let connections = TerminalConnections::default();
        let first = TerminalSubscription {
            id: "first".to_owned(),
            generation: 1,
        };
        connections
            .begin_connect("session", first.clone())
            .expect("first subscription");
        let second = TerminalSubscription {
            id: "second".to_owned(),
            generation: 2,
        };
        connections
            .begin_connect("session", second.clone())
            .expect("replacement subscription");

        let stale_sent = connections.with_current_subscription("session", &first, || true);
        let current_sent = connections.with_current_subscription("session", &second, || true);

        assert_eq!(stale_sent, None);
        assert_eq!(current_sent, Some(true));
    }

    #[test]
    fn refresh_gate_buffers_old_lane_data_until_outcome() {
        let mut gate = TerminalRefreshGate::default();
        gate.begin();
        gate.buffered_old_data.push_back(
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"kept".as_slice().into(),
            ),
        );

        let buffered = gate.take_failed_refresh_data();
        assert_eq!(buffered.len(), 1);
        assert_eq!(buffered[0].sequence, 7);
        assert_eq!(buffered[0].bytes.as_ref(), b"kept");
        assert!(!gate.in_progress);

        gate.begin();
        gate.buffered_old_data.push_back(
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                8,
                b"checkpoint-covered".as_slice().into(),
            ),
        );
        gate.finish_success();
        assert!(gate.buffered_old_data.is_empty());
        assert!(!gate.in_progress);
    }

    #[test]
    fn refresh_requires_exact_active_subscription_and_coalesces() {
        let connections = TerminalConnections::default();
        let subscription = TerminalSubscription {
            id: "current".to_owned(),
            generation: 300,
        };
        let stale_subscription = TerminalSubscription {
            id: "stale".to_owned(),
            generation: 299,
        };
        let (generation, _, _) = connections
            .begin_connect("session", subscription.clone())
            .unwrap();
        let refresh = Arc::new(tokio::sync::Notify::new());
        assert!(connections.install(
            "session",
            generation,
            tokio_util::sync::CancellationToken::new(),
            Arc::clone(&refresh),
        ));
        let completion = connections
            .finish_connect_if_current("session", generation)
            .unwrap();
        assert_eq!(completion.event.result, KODOSI_FFI_OK);
        assert!(!connections.request_refresh("session", &stale_subscription));
        assert!(connections.request_refresh("session", &subscription));
        assert!(connections.request_refresh("session", &subscription));
        let (active_subscription, completion_pending, refresh_requested) = {
            let inner = lock_poison_ok(&connections.inner);
            let state = inner.states.get("session").unwrap();
            let values = (
                state.subscription.clone(),
                state.completion_pending,
                state.refresh_requested,
            );
            drop(inner);
            values
        };
        assert_eq!(active_subscription, subscription);
        assert!(!completion_pending);
        assert!(!refresh_requested);
    }

    #[tokio::test]
    async fn refresh_requested_before_connect_install_is_delivered_after_install() {
        let connections = TerminalConnections::default();
        let subscription = TerminalSubscription {
            id: "pending".to_owned(),
            generation: 400,
        };
        let (generation, _, _) = connections
            .begin_connect("session", subscription.clone())
            .unwrap();
        assert!(connections.request_refresh("session", &subscription));

        let refresh = Arc::new(tokio::sync::Notify::new());
        assert!(connections.install(
            "session",
            generation,
            tokio_util::sync::CancellationToken::new(),
            Arc::clone(&refresh),
        ));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), refresh.notified())
                .await
                .is_ok()
        );
        let (refresh_requested, refresh_installed) = {
            let inner = lock_poison_ok(&connections.inner);
            let state = inner.states.get("session").unwrap();
            let values = (state.refresh_requested, state.refresh.is_some());
            drop(inner);
            values
        };
        assert!(!refresh_requested);
        assert!(refresh_installed);
    }

    #[tokio::test]
    async fn refresh_requested_after_install_is_delivered_before_connect_completion() {
        let connections = TerminalConnections::default();
        let subscription = TerminalSubscription {
            id: "installed".to_owned(),
            generation: 401,
        };
        let (generation, _, _) = connections
            .begin_connect("session", subscription.clone())
            .unwrap();
        let refresh = Arc::new(tokio::sync::Notify::new());
        assert!(connections.install(
            "session",
            generation,
            tokio_util::sync::CancellationToken::new(),
            Arc::clone(&refresh),
        ));

        assert!(connections.request_refresh("session", &subscription));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), refresh.notified())
                .await
                .is_ok()
        );
        let completion = connections
            .finish_connect_if_current("session", generation)
            .unwrap();
        assert_eq!(completion.event.result, KODOSI_FFI_OK);
    }

    #[test]
    fn terminal_subscription_generation_must_increase_per_session() {
        let connections = TerminalConnections::default();
        let current = TerminalSubscription {
            id: "current".to_owned(),
            generation: 12,
        };
        assert!(connections.begin_connect("session", current).is_some());
        for generation in [11, 12] {
            assert!(
                connections
                    .begin_connect(
                        "session",
                        TerminalSubscription {
                            id: "stale".to_owned(),
                            generation,
                        },
                    )
                    .is_none()
            );
        }
        assert!(
            connections
                .begin_connect(
                    "different-session",
                    TerminalSubscription {
                        id: "independent".to_owned(),
                        generation: 1,
                    },
                )
                .is_some()
        );
    }

    #[test]
    fn newer_external_generation_wins_when_its_registration_finishes_second() {
        let connections = std::sync::Arc::new(TerminalConnections::default());
        let newer_started = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release_newer = std::sync::Arc::new(std::sync::Barrier::new(2));
        let newer_connections = std::sync::Arc::clone(&connections);
        let newer_started_thread = std::sync::Arc::clone(&newer_started);
        let release_newer_thread = std::sync::Arc::clone(&release_newer);
        let newer = std::thread::spawn(move || {
            newer_started_thread.wait();
            release_newer_thread.wait();
            newer_connections.begin_connect(
                "session-reversed",
                TerminalSubscription {
                    id: "newer".to_owned(),
                    generation: 11,
                },
            )
        });
        newer_started.wait();

        let older = connections
            .begin_connect(
                "session-reversed",
                TerminalSubscription {
                    id: "older".to_owned(),
                    generation: 10,
                },
            )
            .expect("older registration acquires the lock first");
        assert_eq!(older.0, 1);
        release_newer.wait();
        let newer = newer
            .join()
            .expect("newer registration thread")
            .expect("newer external generation must not be burned");

        assert_eq!(newer.0, 2);
        assert_eq!(
            connections.current_subscription("session-reversed"),
            Some(TerminalSubscription {
                id: "newer".to_owned(),
                generation: 11,
            })
        );
    }

    #[test]
    fn terminal_connect_completion_is_consumed_exactly_once_and_stop_fails_pending() {
        let connections = TerminalConnections::default();
        let subscription = TerminalSubscription {
            id: "subscription".to_owned(),
            generation: 1,
        };
        let (generation, _, _) = connections
            .begin_connect("session", subscription.clone())
            .expect("accepted");

        let completion = connections
            .finish_connect_if_current("session", generation)
            .expect("first completion");
        assert_eq!(completion.event.session_id, "session");
        assert_eq!(completion.event.subscription, subscription);
        assert_eq!(completion.event.result, KODOSI_FFI_OK);
        assert!(
            connections
                .finish_connect_if_current("session", generation)
                .is_none()
        );
        assert!(connections.close_and_cancel_all().is_empty());
        assert!(
            connections
                .begin_connect(
                    "late",
                    TerminalSubscription {
                        id: "late".to_owned(),
                        generation: 1,
                    },
                )
                .is_none(),
            "registration must stay closed after stop drains"
        );

        let connections = TerminalConnections::default();
        let pending = TerminalSubscription {
            id: "pending".to_owned(),
            generation: 2,
        };
        connections
            .begin_connect("session", pending.clone())
            .expect("accepted");
        let completions = connections.close_and_cancel_all();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].event.session_id, "session");
        assert_eq!(completions[0].event.subscription, pending);
        assert_eq!(completions[0].event.result, KODOSI_FFI_RUNTIME_STOPPED);
    }

    #[test]
    fn stop_close_and_connect_registration_are_atomic() {
        for iteration in 1..=200 {
            let connections = std::sync::Arc::new(TerminalConnections::default());
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
            let begin_connections = std::sync::Arc::clone(&connections);
            let begin_barrier = std::sync::Arc::clone(&barrier);
            let begin = std::thread::spawn(move || {
                begin_barrier.wait();
                begin_connections.begin_connect(
                    "session-race",
                    TerminalSubscription {
                        id: format!("subscription-{iteration}"),
                        generation: iteration,
                    },
                )
            });
            let close_connections = std::sync::Arc::clone(&connections);
            let close_barrier = std::sync::Arc::clone(&barrier);
            let close = std::thread::spawn(move || {
                close_barrier.wait();
                close_connections.close_and_cancel_all()
            });
            barrier.wait();
            let accepted = begin.join().expect("connect registration thread");
            let pending = close.join().expect("stop close thread");

            let inner = lock_poison_ok(&connections.inner);
            assert!(!inner.registration_open);
            assert!(inner.states.is_empty());
            drop(inner);
            if let Some((_, subscription, _)) = accepted {
                assert_eq!(pending.len(), 1);
                assert_eq!(pending[0].event.session_id, "session-race");
                assert_eq!(pending[0].event.subscription, subscription);
                assert_eq!(pending[0].event.result, KODOSI_FFI_RUNTIME_STOPPED);
            } else {
                assert!(pending.is_empty());
            }
        }
    }

    #[test]
    fn stop_between_registration_and_task_spawn_returns_accepted_and_completes_once() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let (generation, _, _) = runtime
            .terminal_connections
            .begin_connect(
                "session-stop-before-spawn",
                TerminalSubscription {
                    id: "subscription-stop-before-spawn".to_owned(),
                    generation: 13,
                },
            )
            .expect("registered connect");
        let completions = runtime.terminal_connections.close_and_cancel_all();
        assert_eq!(completions.len(), 1);
        assert_eq!(
            terminal_connect_spawn_failure_result(
                &runtime.terminal_connections,
                "session-stop-before-spawn",
                generation,
            ),
            KODOSI_FFI_OK,
            "the stop-owned callback makes the connect accepted"
        );
        dispatch_terminal_connect_completion(
            completions.into_iter().next().expect("stop completion"),
        );

        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime)),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        let capture = lock_poison_ok(&capture);
        assert_eq!(capture.connect_result_count, 1);
        assert_eq!(
            capture.connect_result,
            Some((
                "session-stop-before-spawn".to_owned(),
                "subscription-stop-before-spawn".to_owned(),
                13,
                KODOSI_FFI_RUNTIME_STOPPED,
            ))
        );
        drop(capture);
    }

    #[test]
    fn task_spawn_rejection_without_claimed_completion_returns_stopped_and_no_callback() {
        let connections = TerminalConnections::default();
        let (generation, _, _) = connections
            .begin_connect(
                "session-unaccepted",
                TerminalSubscription {
                    id: "subscription-unaccepted".to_owned(),
                    generation: 14,
                },
            )
            .expect("registered connect");

        assert_eq!(
            terminal_connect_spawn_failure_result(&connections, "session-unaccepted", generation,),
            KODOSI_FFI_RUNTIME_STOPPED
        );
        assert!(connections.close_and_cancel_all().is_empty());
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct CallbackCapture {
        data: Option<(String, String, u64, u64, Vec<u8>)>,
        control: Option<(String, String, u64)>,
        control_json: Option<Vec<u8>>,
        checkpoint: Option<(String, String, u64, u64, u16, u16, Vec<u8>)>,
        checkpoint_result: i32,
        terminal_events: Vec<String>,
        connect_result: Option<(String, String, u64, i32)>,
        connect_results: Vec<(String, String, u64, i32)>,
        connect_result_count: usize,
        connect_result_thread: Option<std::thread::ThreadId>,
        connect_result_threads: Vec<std::thread::ThreadId>,
    }

    unsafe extern "C" fn capture_data_v2(
        session_id: *const c_char,
        subscription_id: *const c_char,
        generation: u64,
        sequence: u64,
        bytes: *const u8,
        len: usize,
        userdata: *mut c_void,
    ) {
        let (session_id, subscription_id, bytes, capture) = unsafe {
            (
                CStr::from_ptr(session_id).to_string_lossy().into_owned(),
                CStr::from_ptr(subscription_id)
                    .to_string_lossy()
                    .into_owned(),
                std::slice::from_raw_parts(bytes, len).to_vec(),
                &*userdata.cast::<Mutex<CallbackCapture>>(),
            )
        };
        let mut capture = lock_poison_ok(capture);
        capture.terminal_events.push(format!("Data({sequence})"));
        capture.data = Some((session_id, subscription_id, generation, sequence, bytes));
    }

    unsafe extern "C" fn capture_control_v2(
        session_id: *const c_char,
        subscription_id: *const c_char,
        generation: u64,
        json: *const u8,
        len: usize,
        userdata: *mut c_void,
    ) {
        let (session_id, subscription_id, json, capture) = unsafe {
            (
                CStr::from_ptr(session_id).to_string_lossy().into_owned(),
                CStr::from_ptr(subscription_id)
                    .to_string_lossy()
                    .into_owned(),
                std::slice::from_raw_parts(json, len).to_vec(),
                &*userdata.cast::<Mutex<CallbackCapture>>(),
            )
        };
        let mut capture = lock_poison_ok(capture);
        let event_type = serde_json::from_slice::<serde_json::Value>(&json)
            .ok()
            .and_then(|value| {
                value
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "invalid".to_owned());
        let boundary = serde_json::from_slice::<serde_json::Value>(&json)
            .ok()
            .and_then(|value| {
                value
                    .get("finalSequence")
                    .or_else(|| value.get("at_sequence"))
                    .and_then(serde_json::Value::as_u64)
            });
        capture.terminal_events.push(match boundary {
            Some(boundary) => format!("{event_type}({boundary})"),
            None => event_type,
        });
        capture.control = Some((session_id, subscription_id, generation));
        capture.control_json = Some(json);
    }

    unsafe extern "C" fn capture_checkpoint_v2(
        session_id: *const c_char,
        subscription_id: *const c_char,
        generation: u64,
        next_sequence: u64,
        rows: u16,
        cols: u16,
        semantic_json: *const u8,
        semantic_json_len: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let (session_id, subscription_id, semantic_json, capture) = unsafe {
            (
                CStr::from_ptr(session_id).to_string_lossy().into_owned(),
                CStr::from_ptr(subscription_id)
                    .to_string_lossy()
                    .into_owned(),
                std::slice::from_raw_parts(semantic_json, semantic_json_len).to_vec(),
                &*userdata.cast::<Mutex<CallbackCapture>>(),
            )
        };
        let mut capture = lock_poison_ok(capture);
        capture.terminal_events.push("Checkpoint".to_owned());
        capture.checkpoint = Some((
            session_id,
            subscription_id,
            generation,
            next_sequence,
            rows,
            cols,
            semantic_json,
        ));
        capture.checkpoint_result
    }

    unsafe extern "C" fn capture_connect_result_v2(
        session_id: *const c_char,
        subscription_id: *const c_char,
        generation: u64,
        result: i32,
        userdata: *mut c_void,
    ) {
        let (session_id, subscription_id, capture) = unsafe {
            (
                CStr::from_ptr(session_id).to_string_lossy().into_owned(),
                CStr::from_ptr(subscription_id)
                    .to_string_lossy()
                    .into_owned(),
                &*userdata.cast::<Mutex<CallbackCapture>>(),
            )
        };
        let mut capture = lock_poison_ok(capture);
        let completion = (session_id, subscription_id, generation, result);
        capture.connect_result = Some(completion.clone());
        capture.connect_results.push(completion);
        capture.connect_result_count = capture.connect_result_count.saturating_add(1);
        let thread_id = std::thread::current().id();
        capture.connect_result_thread = Some(thread_id);
        capture.connect_result_threads.push(thread_id);
    }

    #[test]
    fn correlated_terminal_callbacks_echo_exact_subscription() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let context = SendCallbackCtx {
            ud: SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
            admission: Arc::new(CallbackAdmission::open()),
            c_sid: c"session-1".to_owned(),
            c_subscription_id: c"subscription-1".to_owned(),
            subscription_generation: 42,
        };
        emit_subscriber_data(
            &callbacks,
            &context,
            &kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"hello".as_slice().into(),
            ),
        );
        emit_subscriber_control(
            &callbacks,
            &context,
            &kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
                rows: 24,
                cols: 80,
                at_sequence: 8,
            },
        );

        assert_eq!(
            *lock_poison_ok(&capture),
            CallbackCapture {
                data: Some((
                    "session-1".to_owned(),
                    "subscription-1".to_owned(),
                    42,
                    7,
                    b"hello".to_vec(),
                )),
                control: Some(("session-1".to_owned(), "subscription-1".to_owned(), 42)),
                control_json: Some(
                    br#"{"at_sequence":8,"cols":80,"rows":24,"type":"Resize"}"#.to_vec(),
                ),
                checkpoint: None,
                checkpoint_result: KODOSI_FFI_OK,
                terminal_events: vec!["Data(7)".to_owned(), "Resize(8)".to_owned()],
                connect_result: None,
                connect_results: Vec::new(),
                connect_result_count: 0,
                connect_result_thread: None,
                connect_result_threads: Vec::new(),
            }
        );
    }

    fn checkpoint_for_test(bytes: &[u8]) -> kodosi_domain::terminal::TerminalCheckpointV2 {
        kodosi_domain::terminal::TerminalCheckpointV2::new(
            kodosi_domain::terminal::TerminalSize::new(24, 80).expect("valid size"),
            kodosi_domain::terminal::TerminalScreen::Primary,
            bytes.to_vec(),
            3,
            4,
            false,
        )
        .expect("valid semantic checkpoint envelope")
    }

    #[test]
    fn local_checkpoint_callback_receives_exact_borrowed_payload_and_metadata() {
        let capture = Mutex::new(CallbackCapture {
            checkpoint_result: KODOSI_FFI_OK,
            ..CallbackCapture::default()
        });
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_semantic_checkpoint_v2 = Some(capture_checkpoint_v2);
        let context = SendCallbackCtx {
            ud: SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
            admission: Arc::new(CallbackAdmission::open()),
            c_sid: c"session-checkpoint".to_owned(),
            c_subscription_id: c"subscription-checkpoint".to_owned(),
            subscription_generation: 17,
        };
        let checkpoint = checkpoint_for_test(br#"{"schemaVersion":1}"#);

        assert_eq!(
            emit_local_checkpoint(&callbacks, &context, &checkpoint, 41),
            KODOSI_FFI_OK
        );
        let capture = lock_poison_ok(&capture);
        assert_eq!(capture.terminal_events, ["Checkpoint"]);
        assert_eq!(
            capture.checkpoint,
            Some((
                "session-checkpoint".to_owned(),
                "subscription-checkpoint".to_owned(),
                17,
                41,
                24,
                80,
                br#"{"schemaVersion":1}"#.to_vec(),
            ))
        );
        drop(capture);
    }

    #[test]
    fn checkpoint_rejection_fails_bootstrap_without_forwarding_data() {
        let capture = Mutex::new(CallbackCapture {
            checkpoint_result: KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED,
            ..CallbackCapture::default()
        });
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_semantic_checkpoint_v2 = Some(capture_checkpoint_v2);
        let context = SendCallbackCtx {
            ud: SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
            admission: Arc::new(CallbackAdmission::open()),
            c_sid: c"session-rejected".to_owned(),
            c_subscription_id: c"subscription-rejected".to_owned(),
            subscription_generation: 18,
        };
        let (data_tx, data_rx) = tokio::sync::mpsc::channel(1);
        let (control_tx, control_rx) = tokio::sync::mpsc::channel(1);
        control_tx
            .try_send(
                kodosi_runtime::terminal_transport::TerminalControlFrame::SemanticCheckpoint {
                    checkpoint: checkpoint_for_test(br#"{"schemaVersion":1}"#),
                    next_sequence: 7,
                },
            )
            .expect("checkpoint queued");
        data_tx
            .try_send(kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"must-not-forward".as_slice().into(),
            ))
            .expect("data queued");
        let mut subscriber = kodosi_runtime::terminal_transport::hub::SubscriberHandle {
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            runtime_incarnation_id: None,
            surface: TerminalSurface::Desktop,
            capability: TerminalCapability::Write,
            data_rx,
            control_rx,
        };
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

        assert_eq!(
            runtime.block_on(consume_subscriber_bootstrap(
                &mut subscriber,
                &callbacks,
                &context,
            )),
            Err(KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED)
        );
        assert_eq!(lock_poison_ok(&capture).terminal_events, ["Checkpoint"]);
        assert_eq!(
            subscriber
                .data_rx
                .try_recv()
                .expect("data remains gated")
                .sequence,
            7
        );
    }

    #[test]
    fn terminal_connect_result_callback_echoes_exact_subscription_generation() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);

        invoke_terminal_connect_result_callback(
            &callbacks,
            &CallbackAdmission::open(),
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
            "session-3",
            &TerminalSubscription {
                id: "subscription-3".to_owned(),
                generation: 91,
            },
            KODOSI_FFI_SESSION_NOT_FOUND,
        );

        assert_eq!(
            lock_poison_ok(&capture).connect_result,
            Some((
                "session-3".to_owned(),
                "subscription-3".to_owned(),
                91,
                KODOSI_FFI_SESSION_NOT_FOUND,
            ))
        );
    }

    #[test]
    fn stop_fails_pending_terminal_connect_completion_before_returning() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let caller_thread = std::thread::current().id();
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        runtime
            .terminal_connections
            .begin_connect(
                "session-stop",
                TerminalSubscription {
                    id: "subscription-stop".to_owned(),
                    generation: 7,
                },
            )
            .expect("pending connect");
        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime)),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        let capture = lock_poison_ok(&capture);
        assert_eq!(
            capture.connect_result,
            Some((
                "session-stop".to_owned(),
                "subscription-stop".to_owned(),
                7,
                KODOSI_FFI_RUNTIME_STOPPED,
            ))
        );
        assert_eq!(capture.connect_result_count, 1);
        assert_ne!(capture.connect_result_thread, Some(caller_thread));
        drop(capture);
    }

    #[test]
    fn disconnect_completion_runs_on_callback_lane_not_ffi_caller() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let caller_thread = std::thread::current().id();
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        runtime
            .terminal_connections
            .begin_connect(
                "session-disconnect",
                TerminalSubscription {
                    id: "subscription-disconnect".to_owned(),
                    generation: 9,
                },
            )
            .expect("pending connect");
        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime)),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();

        let result = unsafe {
            kodosi_terminal_disconnect_v2(
                handle_ptr,
                c"session-disconnect".as_ptr(),
                c"subscription-disconnect".as_ptr(),
                9,
            )
        };
        assert_eq!(result, KODOSI_FFI_OK);
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        let capture = lock_poison_ok(&capture);
        assert_eq!(
            capture.connect_result,
            Some((
                "session-disconnect".to_owned(),
                "subscription-disconnect".to_owned(),
                9,
                KODOSI_FFI_SESSION_NOT_FOUND,
            ))
        );
        assert_eq!(capture.connect_result_count, 1);
        assert_ne!(capture.connect_result_thread, Some(caller_thread));
        drop(capture);
    }

    #[test]
    fn reserved_completion_survives_stop_closing_the_callback_sender() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let subscription = TerminalSubscription {
            id: "subscription-reserved".to_owned(),
            generation: 12,
        };
        runtime
            .terminal_connections
            .begin_connect("session-reserved", subscription.clone())
            .expect("pending connect");
        let (removed, reservation) = runtime
            .terminal_connections
            .disconnect("session-reserved", &subscription);
        assert!(removed);
        let reservation = reservation.expect("callback sender reserved before removal");

        assert!(
            runtime
                .terminal_connections
                .close_and_cancel_all()
                .is_empty()
        );
        dispatch_terminal_connect_completion(reservation);

        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime)),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        let capture = lock_poison_ok(&capture);
        assert_eq!(
            capture.connect_results,
            vec![(
                "session-reserved".to_owned(),
                "subscription-reserved".to_owned(),
                12,
                KODOSI_FFI_SESSION_NOT_FOUND,
            )]
        );
        drop(capture);
    }

    #[test]
    fn supersede_completion_runs_on_callback_lane_not_ffi_caller() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let caller_thread = std::thread::current().id();
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        runtime
            .terminal_connections
            .begin_connect(
                "session-supersede",
                TerminalSubscription {
                    id: "subscription-old".to_owned(),
                    generation: 10,
                },
            )
            .expect("first pending connect");
        let (_, _, superseded) = runtime
            .terminal_connections
            .begin_connect(
                "session-supersede",
                TerminalSubscription {
                    id: "subscription-new".to_owned(),
                    generation: 11,
                },
            )
            .expect("replacement connect");
        dispatch_superseded_terminal_connect_result(superseded);

        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime)),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        let capture = lock_poison_ok(&capture);
        assert_eq!(
            capture.connect_results,
            vec![
                (
                    "session-supersede".to_owned(),
                    "subscription-old".to_owned(),
                    10,
                    KODOSI_FFI_STALE_SUBSCRIPTION,
                ),
                (
                    "session-supersede".to_owned(),
                    "subscription-new".to_owned(),
                    11,
                    KODOSI_FFI_RUNTIME_STOPPED,
                ),
            ]
        );
        assert_eq!(capture.connect_result_count, 2);
        assert!(
            capture
                .connect_result_threads
                .iter()
                .all(|thread| *thread != caller_thread)
        );
        drop(capture);
    }

    #[test]
    fn immediate_terminal_close_still_completes_connect_success_exactly_once() {
        let _guard = lock_poison_ok(runtime_state_test_lock());
        let caller_thread = std::thread::current().id();
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_connect_result_v2 = Some(capture_connect_result_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let subscription = TerminalSubscription {
            id: "subscription-close".to_owned(),
            generation: 8,
        };
        let (generation, subscription, _) = runtime
            .terminal_connections
            .begin_connect("session-close", subscription)
            .expect("connect registration");
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let refresh = Arc::new(tokio::sync::Notify::new());
        assert!(runtime.terminal_connections.install(
            "session-close",
            generation,
            cancel_token.clone(),
            Arc::clone(&refresh),
        ));
        let (_data_tx, data_rx) = tokio::sync::mpsc::channel(1);
        let (control_tx, control_rx) = tokio::sync::mpsc::channel(1);
        control_tx
            .try_send(
                kodosi_runtime::terminal_transport::TerminalControlFrame::Closed {
                    reason: kodosi_runtime::terminal_transport::TerminalCloseReason::SessionEnded,
                    final_sequence: 0,
                },
            )
            .expect("queue immediate close");
        let start_drain = spawn_terminal_subscriber_drain_from_task(
            "session-close",
            kodosi_runtime::terminal_transport::hub::SubscriberHandle {
                connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
                runtime_incarnation_id: None,
                surface: TerminalSurface::Desktop,
                capability: TerminalCapability::Write,
                data_rx,
                control_rx,
            },
            cancel_token,
            refresh,
            generation,
            SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: c"session-close".to_owned(),
                c_subscription_id: std::ffi::CString::new(subscription.id.as_str())
                    .expect("subscription id"),
                subscription_generation: subscription.generation,
            },
            0,
            &runtime,
        )
        .expect("drain task registered");
        let completed = runtime
            .terminal_connections
            .finish_connect_if_current("session-close", generation)
            .expect("success completion reserved before drain starts");
        dispatch_terminal_connect_completion(completed);
        start_drain.send(()).expect("release drain");
        runtime.tokio_rt.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });
        assert!(
            runtime
                .terminal_connections
                .current_subscription("session-close")
                .is_none(),
            "immediate close should clear the connected registry entry"
        );

        let handle = FfiHandle {
            runtime: std::sync::Mutex::new(Some(runtime)),
        };
        let handle_ptr = std::ptr::from_ref(&handle).cast_mut().cast::<c_void>();
        assert!(try_open_lifecycle_gate());

        unsafe { kodosi_stop(handle_ptr) };

        let capture = lock_poison_ok(&capture);
        assert_eq!(
            capture.connect_result,
            Some((
                "session-close".to_owned(),
                "subscription-close".to_owned(),
                8,
                KODOSI_FFI_OK,
            ))
        );
        assert_eq!(capture.connect_result_count, 1);
        assert_ne!(capture.connect_result_thread, Some(caller_thread));
        assert_eq!(capture.terminal_events, ["Closed(0)"]);
        drop(capture);
    }

    #[test]
    fn failed_refresh_replay_drains_resize_boundary_before_next_data() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (_data_tx, data_rx) = tokio::sync::mpsc::channel(2);
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(2);
        let mut refresh_gate = TerminalRefreshGate::default();
        refresh_gate.begin();
        refresh_gate.buffered_old_data.extend([
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"seven".as_slice().into(),
            ),
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                8,
                b"eight".as_slice().into(),
            ),
        ]);
        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("session id"),
                c_subscription_id: c"failed-refresh-boundary".to_owned(),
                subscription_generation: 3,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: Some(7),
            pending_resizes: VecDeque::from([PendingTerminalResize {
                rows: 40,
                cols: 120,
                at_sequence: 7,
            }]),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate,
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        assert!(runtime.tokio_rt.block_on(drain.finish_refresh(None)));
        assert!(!drain.shutdown_requested);
        assert!(!drain.integrity_close_emitted);
        assert_eq!(drain.next_data_sequence, Some(9));
        assert_eq!(
            lock_poison_ok(&capture).terminal_events,
            ["Resize(7)", "Data(7)", "Data(8)"]
        );
    }

    #[test]
    fn ffi_resize_at_current_cursor_does_not_wait_for_more_output() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (_data_tx, data_rx) = tokio::sync::mpsc::channel(2);
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(2);
        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("session id"),
                c_subscription_id: c"resize-boundary".to_owned(),
                subscription_generation: 3,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: Some(7),
            pending_resizes: VecDeque::new(),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate: TerminalRefreshGate::default(),
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        drain.handle_control(Some(
            kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
                rows: 40,
                cols: 120,
                at_sequence: 7,
            },
        ));
        assert!(lock_poison_ok(&capture).terminal_events.is_empty());
        assert!(!drain.resolve_pending_data());
        assert_eq!(lock_poison_ok(&capture).terminal_events, ["Resize(7)"]);
        assert!(drain.resolve_pending_data());
        drain.handle_data(Some(
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"after-resize".as_slice().into(),
            ),
        ));
        assert!(drain.resolve_pending_data());

        assert_eq!(
            lock_poison_ok(&capture).terminal_events,
            ["Resize(7)", "Data(7)"]
        );
    }

    #[test]
    fn ffi_consecutive_resizes_at_same_boundary_preserve_arrival_order() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (_data_tx, data_rx) = tokio::sync::mpsc::channel(2);
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(2);
        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("session id"),
                c_subscription_id: c"resize-boundary".to_owned(),
                subscription_generation: 3,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: Some(7),
            pending_resizes: VecDeque::new(),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate: TerminalRefreshGate::default(),
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        for (rows, cols) in [(40, 120), (41, 121)] {
            drain.handle_control(Some(
                kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
                    rows,
                    cols,
                    at_sequence: 7,
                },
            ));
        }
        drain.handle_data(Some(
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"after-resizes".as_slice().into(),
            ),
        ));
        assert!(!drain.resolve_pending_data());
        assert!(!drain.resolve_pending_data());
        assert!(drain.resolve_pending_data());

        assert_eq!(
            lock_poison_ok(&capture).terminal_events,
            ["Resize(7)", "Resize(7)", "Data(7)"]
        );
    }

    #[test]
    fn ffi_resize_data_before_control_emits_at_exact_boundary() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (_data_tx, data_rx) = tokio::sync::mpsc::channel(2);
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(2);
        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("session id"),
                c_subscription_id: c"resize-boundary".to_owned(),
                subscription_generation: 3,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: Some(7),
            pending_resizes: VecDeque::new(),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate: TerminalRefreshGate::default(),
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        drain.handle_data(Some(
            kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"after-resize".as_slice().into(),
            ),
        ));
        drain.handle_control(Some(
            kodosi_runtime::terminal_transport::TerminalControlFrame::Resize {
                rows: 40,
                cols: 120,
                at_sequence: 7,
            },
        ));
        assert!(!drain.resolve_pending_data());
        assert!(drain.resolve_pending_data());

        assert_eq!(
            lock_poison_ok(&capture).terminal_events,
            ["Resize(7)", "Data(7)"]
        );
    }

    #[test]
    fn terminal_close_waits_for_every_prequeued_data_frame() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (data_tx, data_rx) = tokio::sync::mpsc::channel(4);
        let (control_tx, control_rx) = tokio::sync::mpsc::channel(4);
        data_tx
            .try_send(kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                7,
                b"a".as_slice().into(),
            ))
            .expect("data 7");
        data_tx
            .try_send(kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                8,
                b"b".as_slice().into(),
            ))
            .expect("data 8");
        control_tx
            .try_send(
                kodosi_runtime::terminal_transport::TerminalControlFrame::Closed {
                    reason: kodosi_runtime::terminal_transport::TerminalCloseReason::SessionEnded,
                    final_sequence: 9,
                },
            )
            .expect("close");

        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("session id"),
                c_subscription_id: c"ordered-close".to_owned(),
                subscription_generation: 3,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: Some(7),
            pending_resizes: VecDeque::new(),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate: TerminalRefreshGate::default(),
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        runtime.tokio_rt.block_on(async {
            assert!(drain.advance().await, "close held as barrier");
            assert!(drain.advance().await, "first final data forwarded");
            assert!(drain.advance().await, "second final data forwarded");
            assert!(!drain.advance().await, "close emitted after boundary");
        });

        assert_eq!(
            lock_poison_ok(&capture).terminal_events,
            ["Data(7)", "Data(8)", "Closed(9)"]
        );
    }

    #[test]
    fn terminal_sequence_gap_emits_one_correlated_integrity_close() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_data_v2 = Some(capture_data_v2);
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (data_tx, data_rx) = tokio::sync::mpsc::channel(2);
        let (control_tx, control_rx) = tokio::sync::mpsc::channel(2);
        data_tx
            .try_send(kodosi_runtime::terminal_transport::TerminalDataFrame::new(
                8,
                b"gap".as_slice().into(),
            ))
            .expect("gapped data");
        control_tx
            .try_send(
                kodosi_runtime::terminal_transport::TerminalControlFrame::Closed {
                    reason: kodosi_runtime::terminal_transport::TerminalCloseReason::SessionEnded,
                    final_sequence: 9,
                },
            )
            .expect("close");
        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("session id"),
                c_subscription_id: c"gapped-close".to_owned(),
                subscription_generation: 4,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: Some(7),
            pending_resizes: VecDeque::new(),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate: TerminalRefreshGate::default(),
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        runtime.tokio_rt.block_on(async {
            assert!(drain.advance().await, "close held as barrier");
            assert!(drain.advance().await, "gap records invariant failure");
            assert!(!drain.advance().await, "gapped drain terminates");
        });

        let captured = lock_poison_ok(&capture);
        assert_eq!(captured.terminal_events, ["Closed(7)"]);
        let json: serde_json::Value = serde_json::from_slice(
            captured
                .control_json
                .as_deref()
                .expect("integrity close JSON"),
        )
        .expect("valid close JSON");
        assert_eq!(json["type"], "Closed");
        assert_eq!(json["finalSequence"], 7);
        assert!(
            json["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("sequence gap"))
        );
        drop(captured);
        drain.fail_closed("duplicate");
        assert_eq!(lock_poison_ok(&capture).terminal_events, ["Closed(7)"]);
    }

    #[test]
    fn exhausted_resubscribe_emits_closed_instead_of_freezing() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let runtime = runtime_shell_with(
            callbacks,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
        );
        let session_id = kodosi_domain::ids::SessionId::new();
        let (data_tx, data_rx) = tokio::sync::mpsc::channel(1);
        let (control_tx, control_rx) = tokio::sync::mpsc::channel(1);
        drop(data_tx);
        drop(control_tx);
        let mut drain = TerminalSubscriberDrain {
            sid: session_id.to_string(),
            session_id: Some(session_id),
            connection_id: kodosi_runtime::terminal_transport::TerminalConnectionId::new(),
            data_rx,
            control_rx,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            refresh: Arc::new(tokio::sync::Notify::new()),
            generation: 1,
            ctx: SendCallbackCtx {
                ud: runtime.userdata,
                admission: Arc::clone(&runtime.callback_admission),
                c_sid: std::ffi::CString::new(session_id.to_string()).expect("valid session id"),
                c_subscription_id: c"recovery-subscription".to_owned(),
                subscription_generation: 9,
            },
            runtime: Arc::clone(&runtime),
            data_open: true,
            control_open: true,
            next_data_sequence: None,
            pending_resizes: VecDeque::new(),
            pending_data: None,
            pending_close: None,
            refresh_in_flight: None,
            refresh_gate: TerminalRefreshGate::default(),
            shutdown_requested: false,
            integrity_close_emitted: false,
        };

        runtime.tokio_rt.block_on(async {
            assert!(drain.advance().await, "closed control channel marks detach");
            assert!(!drain.advance().await, "failed recovery stops the drain");
        });

        let capture = lock_poison_ok(&capture);
        let control = capture.control.clone();
        let control_json = capture.control_json.clone();
        drop(capture);
        assert_eq!(
            control,
            Some((
                session_id.to_string(),
                "recovery-subscription".to_owned(),
                9,
            ))
        );
        let json: serde_json::Value =
            serde_json::from_slice(control_json.as_deref().expect("closed control JSON"))
                .expect("valid control JSON");
        assert_eq!(json["type"], "Closed");
        assert_eq!(json["reason"], "relay disconnected");
        assert_eq!(json["finalSequence"], 0);
    }

    #[test]
    fn correlated_notification_uses_current_subscription() {
        let capture = Mutex::new(CallbackCapture::default());
        let mut callbacks = empty_callbacks();
        callbacks.on_terminal_control_v2 = Some(capture_control_v2);
        let connections = TerminalConnections::default();
        connections
            .begin_connect(
                "session-2",
                TerminalSubscription {
                    id: "subscription-2".to_owned(),
                    generation: 77,
                },
            )
            .expect("subscription is current");

        emit_terminal_notification(
            &callbacks,
            &CallbackAdmission::open(),
            &connections,
            SendPtr(std::ptr::from_ref(&capture).cast_mut().cast()),
            "session-2",
            br#"{"type":"terminal.notification"}"#,
        );

        assert_eq!(
            lock_poison_ok(&capture).control,
            Some(("session-2".to_owned(), "subscription-2".to_owned(), 77))
        );
    }
}
