use super::{
    ACTIVE, Arc, AssertUnwindSafe, CStr, CancellationToken, Condvar, Config, Error, Executor, Gate,
    HANDLES, HashMap, JoinSet, KODOSI_FFI_BUSY, KODOSI_FFI_DESER_FAILED, KODOSI_FFI_PANIC,
    KODOSI_FFI_PAYLOAD_TOO_LARGE, KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_SESSION_NOT_FOUND,
    KODOSI_FFI_STALE_SUBSCRIPTION, KODOSI_MAX_FRAME_BYTES, KODOSI_START_ALREADY_ACTIVE,
    KODOSI_START_FAILED, KODOSI_START_HOST_BUSY, KODOSI_START_INVALID_CALLBACKS,
    KODOSI_START_REJECTED, KodosiCallbacks, KodosiStartFailure, Mutex, NEXT_HANDLE, Ordering,
    RuntimeHandle, STOP_BUDGET, State, Subscriptions, UserData, c_char, c_void, local_host, lock,
    record_start_failure, start_failure,
};

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
pub(super) struct Completion {
    pub(super) done: Mutex<bool>,
    changed: Condvar,
}
impl Completion {
    pub(super) fn finish(&self) {
        *lock(&self.done) = true;
        self.changed.notify_all();
    }
    pub(super) fn wait(&self) {
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
pub(super) struct Instance {
    pub(super) runtime: RuntimeHandle,
    pub(super) state: Arc<State>,
    pub(super) executor: tokio::runtime::Handle,
    pub(super) tasks: Mutex<Option<JoinSet<()>>>,
    pub(super) stopped: Completion,
}
impl Instance {
    pub(super) fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> bool {
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
pub(super) fn handles() -> &'static Mutex<HashMap<usize, Arc<Instance>>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(super) fn instance(handle: *mut c_void) -> Option<Arc<Instance>> {
    if handle.is_null() {
        return None;
    }
    lock(handles())
        .get(&(handle as usize))
        .filter(|instance| !instance.state.cancel.is_cancelled())
        .cloned()
}
pub(super) fn code(error: &Error) -> i32 {
    match error {
        Error::Busy => KODOSI_FFI_BUSY,
        Error::Stopped => KODOSI_FFI_RUNTIME_STOPPED,
        Error::NotFound => KODOSI_FFI_SESSION_NOT_FOUND,
        Error::Stale => KODOSI_FFI_STALE_SUBSCRIPTION,
        _ => KODOSI_FFI_DESER_FAILED,
    }
}
pub(super) fn caught(action: impl FnOnce() -> i32) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(action)).unwrap_or(KODOSI_FFI_PANIC)
}
pub(super) unsafe fn text<'a>(value: *const c_char) -> Option<&'a str> {
    if value.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(value) }.to_str().ok()?;
    (value.len() <= 256).then_some(value)
}
pub(super) unsafe fn payload<'a>(bytes: *const u8, len: usize) -> Result<&'a [u8], i32> {
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
    let started = std::panic::catch_unwind(AssertUnwindSafe(
        || -> std::result::Result<*mut c_void, Box<KodosiStartFailure>> {
            if callbacks.is_null() || callbacks_size != std::mem::size_of::<KodosiCallbacks>() {
                return Err(start_failure(
                    KODOSI_START_INVALID_CALLBACKS,
                    "The runtime callbacks are missing or sized for another ABI.",
                    None,
                ));
            }
            let callbacks = unsafe { callbacks.read() };
            if callbacks.on_event.is_none()
                || callbacks.on_terminal_data.is_none()
                || callbacks.on_terminal_control.is_none()
                || callbacks.on_terminal_connect_result.is_none()
                || callbacks.on_terminal_checkpoint.is_none()
            {
                return Err(start_failure(
                    KODOSI_START_INVALID_CALLBACKS,
                    "A required runtime callback is missing.",
                    None,
                ));
            }
            let Some(active) = ActiveRuntime::acquire() else {
                return Err(start_failure(
                    KODOSI_START_ALREADY_ACTIVE,
                    "This process already runs the Kodosi runtime.",
                    None,
                ));
            };
            let key = NEXT_HANDLE
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
                .map_err(|_| {
                    start_failure(
                        KODOSI_START_FAILED,
                        "The runtime handle space is exhausted.",
                        None,
                    )
                })?;
            let (ready, receive) = std::sync::mpsc::sync_channel(1);
            let userdata = UserData(userdata);
            std::thread::Builder::new()
                .name("kodosi-runtime".into())
                .spawn(move || run_executor(key, callbacks, userdata, ready, active))
                .map_err(|error| start_failure(KODOSI_START_FAILED, &error.to_string(), None))?;
            let (instance, initial, events) = receive.recv().map_err(|_| {
                start_failure(
                    KODOSI_START_FAILED,
                    "The runtime thread ended before reporting.",
                    None,
                )
            })??;
            instance.spawn(pump_events(
                instance.runtime.clone(),
                Arc::clone(&instance.state),
                initial,
                events,
            ));
            Ok(key as *mut c_void)
        },
    ));
    match started {
        Ok(Ok(handle)) => {
            record_start_failure(None);
            handle
        }
        Ok(Err(failure)) => {
            record_start_failure(Some(&failure));
            std::ptr::null_mut()
        }
        Err(_) => {
            record_start_failure(Some(&start_failure(
                KODOSI_START_FAILED,
                "The runtime panicked during startup.",
                None,
            )));
            std::ptr::null_mut()
        }
    }
}

type Started = (
    Arc<Instance>,
    Vec<kodosi_runtime::Event>,
    tokio::sync::broadcast::Receiver<kodosi_runtime::Event>,
);
type Setup = (
    Executor,
    Arc<Instance>,
    Vec<kodosi_runtime::Event>,
    tokio::sync::broadcast::Receiver<kodosi_runtime::Event>,
);

fn run_executor(
    key: usize,
    callbacks: KodosiCallbacks,
    userdata: UserData,
    ready: std::sync::mpsc::SyncSender<std::result::Result<Started, Box<KodosiStartFailure>>>,
    active: ActiveRuntime,
) {
    let setup = std::panic::catch_unwind(AssertUnwindSafe(
        || -> std::result::Result<Setup, Box<KodosiStartFailure>> {
            let executor = Executor::new()
                .map_err(|error| start_failure(KODOSI_START_FAILED, &error.to_string(), None))?;
            let config = Config::load()
                .map_err(|error| start_failure(KODOSI_START_REJECTED, &error.to_string(), None))?;
            let root = config.data_root.clone();
            let runtime = executor
                .block_on(kodosi_runtime::start(config))
                .map_err(|error| {
                    tracing::error!(%error, "could not start embedded runtime");
                    let code = match &error {
                        Error::HostBusy(_) => KODOSI_START_HOST_BUSY,
                        Error::Invalid(_) => KODOSI_START_REJECTED,
                        _ => KODOSI_START_FAILED,
                    };
                    let status = (code == KODOSI_START_HOST_BUSY)
                        .then(|| executor.block_on(local_host::host_status(&root)).ok())
                        .flatten();
                    start_failure(code, &error.to_string(), status)
                })?;
            let instance = Arc::new(Instance {
                runtime,
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
            let (initial, events) =
                executor
                    .block_on(instance.runtime.observe())
                    .map_err(|error| {
                        tracing::error!(%error, "could not observe embedded runtime");
                        start_failure(KODOSI_START_FAILED, &error.to_string(), None)
                    })?;
            Ok((executor, instance, initial, events))
        },
    ));
    let (executor, instance, initial, events) = match setup {
        Ok(Ok(started)) => started,
        Ok(Err(failure)) => {
            drop(active);
            let _ = ready.send(Err(failure));
            return;
        }
        Err(_) => {
            drop(active);
            let _ = ready.send(Err(start_failure(
                KODOSI_START_FAILED,
                "The embedded runtime panicked during startup.",
                None,
            )));
            return;
        }
    };
    lock(handles()).insert(key, Arc::clone(&instance));
    if ready
        .send(Ok((Arc::clone(&instance), initial, events)))
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
        executor.block_on(async {
            let _ = tokio::time::timeout(STOP_BUDGET, instance.runtime.shutdown()).await;
        });
        instance.state.gate.wait();
        executor.block_on(async {
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
