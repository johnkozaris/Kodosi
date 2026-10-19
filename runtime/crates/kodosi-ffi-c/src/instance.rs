use super::{
    ACTIVE, Arc, AssertUnwindSafe, CStr, CancellationToken, Condvar, Error, Executor, Gate,
    HANDLES, HashMap, JoinSet, KODOSI_FFI_BUSY, KODOSI_FFI_DESER_FAILED, KODOSI_FFI_PANIC,
    KODOSI_FFI_PAYLOAD_TOO_LARGE, KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_SESSION_NOT_FOUND,
    KODOSI_FFI_STALE_SUBSCRIPTION, KODOSI_MAX_FRAME_BYTES, KODOSI_START_ALREADY_ACTIVE,
    KODOSI_START_FAILED, KODOSI_START_INVALID_CALLBACKS, KodosiCallbacks, KodosiStartFailure,
    Mutex, NEXT_HANDLE, Ordering, STOP_BUDGET, State, Subscriptions, UserData, c_char, c_void,
    link::{self, Link, Runner},
    lock, record_start_failure, start_failure,
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
    pub(super) link: Link,
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
                ));
            }
            let Some(active) = ActiveRuntime::acquire() else {
                return Err(start_failure(
                    KODOSI_START_ALREADY_ACTIVE,
                    "This process already runs the Kodosi runtime.",
                ));
            };
            let key = NEXT_HANDLE
                .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
                .map_err(|_| {
                    start_failure(
                        KODOSI_START_FAILED,
                        "The runtime handle space is exhausted.",
                    )
                })?;
            let (ready, receive) = std::sync::mpsc::sync_channel(1);
            let userdata = UserData(userdata);
            std::thread::Builder::new()
                .name("kodosi-runtime".into())
                .spawn(move || run_executor(key, callbacks, userdata, ready, active))
                .map_err(|error| start_failure(KODOSI_START_FAILED, &error.to_string()))?;
            let (instance, runner) = receive.recv().map_err(|_| {
                start_failure(
                    KODOSI_START_FAILED,
                    "The runtime thread ended before reporting.",
                )
            })??;
            runner.spawn(&instance);
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
            )));
            std::ptr::null_mut()
        }
    }
}

type Started = (Arc<Instance>, Runner);
type Setup = (Executor, Arc<Instance>, Runner);

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
                .map_err(|error| start_failure(KODOSI_START_FAILED, &error.to_string()))?;
            let config = crate::app::start_config()?;
            kodosi_runtime::diagnostics::install(
                &config.data_root,
                kodosi_runtime::diagnostics::Role::App,
            );
            let (link, runner) = executor.block_on(Link::open(&config))?;
            let instance = Arc::new(Instance {
                link,
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
            Ok((executor, instance, runner))
        },
    ));
    let (executor, instance, runner) = match setup {
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
            )));
            return;
        }
    };
    lock(handles()).insert(key, Arc::clone(&instance));
    if ready.send(Ok((Arc::clone(&instance), runner))).is_err() {
        instance.state.close();
    }
    drop(ready);
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        executor.block_on(async {
            tokio::select! {
                () = instance.state.cancel.cancelled() => {},
                () = link::stopped(&instance.link) => {},
            }
        });
        instance.state.close();
        executor.block_on(async {
            let _ = tokio::time::timeout(STOP_BUDGET, link::shutdown(&instance.link)).await;
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
