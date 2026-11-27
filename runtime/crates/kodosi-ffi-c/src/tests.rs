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
fn start_rejects_missing_or_wrong_callback_table() {
    assert!(unsafe { kodosi_start(std::ptr::null(), 0, std::ptr::null_mut()) }.is_null());
    let callbacks = KodosiCallbacks {
        on_event: None,
        on_terminal_data: None,
        on_terminal_control: None,
        on_terminal_connect_result: None,
        on_terminal_checkpoint: None,
        on_video_frame: None,
        on_video_message: None,
        on_video_repair: None,
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
    _: bool,
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
        on_video_frame: Some(take_frame),
        on_video_message: Some(ignore_message),
        on_video_repair: Some(ignore_repair),
    }
}
unsafe extern "C" fn take_frame(
    _: *const c_char,
    _: *const KodosiVideoFrame,
    _: *mut c_void,
) -> i32 {
    KODOSI_VIDEO_TAKEN
}
unsafe extern "C" fn ignore_message(_: *const c_char, _: *const u8, _: usize, _: *mut c_void) {}
unsafe extern "C" fn ignore_repair(_: *const c_char, _: *mut c_void) {}
fn state() -> State {
    State {
        callbacks: callbacks(),
        userdata: UserData(std::ptr::null_mut()),
        gate: Gate::default(),
        serial: Mutex::new(()),
        cancel: CancellationToken::new(),
        subscriptions: Mutex::new(Subscriptions::default()),
        watches: Mutex::new(HashMap::new()),
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
fn a_callback_of_one_gate_does_not_excuse_waiting_for_another() {
    let runtime = Gate::default();
    let activation = Gate::default();
    {
        let _scope = CallbackScope::enter(&activation);
        assert!(CallbackScope::within(&activation));
        assert!(!CallbackScope::within(&runtime));
        let _nested = CallbackScope::enter(&runtime);
        assert!(CallbackScope::within(&runtime));
    }
    assert!(!CallbackScope::within(&activation));
    assert!(!CallbackScope::within(&runtime));
}

struct HeldCalls {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    count: AtomicUsize,
}
unsafe extern "C" fn held_frame(
    _: *const c_char,
    _: *const KodosiVideoFrame,
    userdata: *mut c_void,
) -> i32 {
    let calls = unsafe { &*userdata.cast::<HeldCalls>() };
    calls.count.fetch_add(1, Ordering::SeqCst);
    let _ = calls.entered.send(());
    let _ = lock(&calls.release).recv();
    KODOSI_VIDEO_TAKEN
}
#[test]
fn no_video_call_runs_once_the_app_ended_its_watch() {
    use kodosi_runtime::video::{Codec, Frame, FrameKind, Shown, Took};
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let (entered, calls_entered) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let calls = HeldCalls {
        entered,
        release: Mutex::new(released),
        count: AtomicUsize::new(0),
    };
    let mut state = state();
    state.callbacks.on_video_frame = Some(held_frame);
    state.userdata = UserData(std::ptr::from_ref(&calls).cast_mut().cast());
    let state = Arc::new(state);
    let watch = Arc::new(video::Watch::new(
        CString::new(Uuid::now_v7().to_string()).unwrap(),
        CancellationToken::new(),
    ));
    let (shown, watched) = tokio::sync::mpsc::channel(4);
    let (took, mut answers) = tokio::sync::mpsc::channel(4);
    executor.spawn(video::serve(
        Arc::clone(&state),
        Arc::clone(&watch),
        watched,
        took,
    ));
    let frame = Frame {
        id: 1,
        codec: Codec::H264,
        kind: FrameKind::Key,
        region: None,
        data: Bytes::from_static(&[0, 0, 0, 1, 0x65]),
    }
    .encode();
    shown.blocking_send(Shown::Frame(frame.clone())).unwrap();
    calls_entered.recv().unwrap();
    let ending = {
        let watch = Arc::clone(&watch);
        std::thread::spawn(move || watch.end())
    };
    while !watch.cancel.is_cancelled() {
        std::thread::yield_now();
    }
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !ending.is_finished(),
        "the watch ends only once the app's call returned, so the app may free its decoder then"
    );
    release.send(()).unwrap();
    ending.join().unwrap();
    assert_eq!(answers.blocking_recv(), Some(Took::Taken));
    drop(shown.try_send(Shown::Frame(frame)));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        calls.count.load(Ordering::SeqCst),
        1,
        "no call after the end"
    );
    drop(state);
    executor.shutdown_timeout(Duration::from_secs(1));
}
