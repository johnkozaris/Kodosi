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
fn retiring_a_subscription_does_not_wait_for_a_stalled_callback() {
    let state = Arc::new(state());
    let token = subscription(Uuid::now_v7(), 1);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let worker = {
        let state = Arc::clone(&state);
        let token = Arc::clone(&token);
        std::thread::spawn(move || {
            state.callback(Some(&token), || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        })
    };
    entered_rx.recv().unwrap();
    token.retire();
    assert!(token.cancel.is_cancelled());
    assert!(token.gate.enter().is_none());
    assert_eq!(lock(&token.gate.state).active, 1);
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap().is_some());
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
