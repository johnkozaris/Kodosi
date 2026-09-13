#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::{Value, json};
use std::{process::Command, sync::mpsc};

#[derive(Debug)]
enum Message {
    Event(Value),
    Connected(i32),
    Checkpoint(u64),
    Data(u64, Vec<u8>),
}
struct Context {
    sender: mpsc::Sender<Message>,
}
unsafe fn context<'a>(data: *mut c_void) -> &'a Context {
    unsafe { &*data.cast::<Context>() }
}
unsafe extern "C" fn event(bytes: *const u8, len: usize, data: *mut c_void) {
    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
    if let Ok(value) = serde_json::from_slice(bytes) {
        drop(unsafe { context(data) }.sender.send(Message::Event(value)));
    }
}
unsafe extern "C" fn output(
    _: *const c_char,
    _: *const c_char,
    _: u64,
    sequence: u64,
    bytes: *const u8,
    len: usize,
    data: *mut c_void,
) {
    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec();
    drop(
        unsafe { context(data) }
            .sender
            .send(Message::Data(sequence, bytes)),
    );
}
unsafe extern "C" fn control(
    _: *const c_char,
    _: *const c_char,
    _: u64,
    _: *const u8,
    _: usize,
    _: *mut c_void,
) {
}
unsafe extern "C" fn connected(
    _: *const c_char,
    _: *const c_char,
    _: u64,
    result: i32,
    data: *mut c_void,
) {
    drop(
        unsafe { context(data) }
            .sender
            .send(Message::Connected(result)),
    );
}
unsafe extern "C" fn checkpoint(
    _: *const c_char,
    _: *const c_char,
    _: u64,
    sequence: u64,
    _: u16,
    _: u16,
    bytes: *const u8,
    len: usize,
    data: *mut c_void,
) -> i32 {
    if bytes.is_null() || len == 0 {
        return KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    }
    drop(
        unsafe { context(data) }
            .sender
            .send(Message::Checkpoint(sequence)),
    );
    KODOSI_FFI_OK
}
fn receive_until<T>(rx: &mpsc::Receiver<Message>, mut test: impl FnMut(Message) -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let message = rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("runtime callback");
        if let Some(value) = test(message) {
            return value;
        }
    }
}
fn send(handle: *mut c_void, mut value: Value) {
    value["accountUserId"] = Value::Null;
    value["accountEpoch"] = json!(0);
    let bytes = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        unsafe { kodosi_send_command(handle, bytes.as_ptr(), bytes.len()) },
        KODOSI_FFI_OK
    );
}
struct Running(*mut c_void);
impl Drop for Running {
    fn drop(&mut self) {
        unsafe { kodosi_stop(self.0) }
    }
}

#[test]
fn embedded_terminal_lifecycle() {
    if std::env::var_os("KODOSI_FFI_LIFECYCLE_CHILD").is_some() {
        exercise_terminal();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "lifecycle_tests::embedded_terminal_lifecycle",
            "--nocapture",
        ])
        .env("KODOSI_FFI_LIFECYCLE_CHILD", "1")
        .env("HOME", &home)
        .env("KODOSI_DATA_ROOT", root.join("test"))
        .env(
            "KODOSI_PRODUCTION_DATA_ROOT",
            root.join("unused-production"),
        )
        .env("KODOSI__BACKEND__API", "http://127.0.0.1:1")
        .env("KODOSI__AUTH__ISSUER", "http://127.0.0.1:1")
        .env("KODOSI__RUNTIME__INITIAL_SHELL", "/bin/sh")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "isolated FFI lifecycle failed: {status}");
            return;
        }
        if std::time::Instant::now() >= deadline {
            drop(child.kill());
            drop(child.wait());
            assert!(
                std::time::Instant::now() < deadline,
                "isolated FFI lifecycle timed out"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct TerminalView {
    handle: *mut c_void,
    session: CString,
    incarnation: CString,
    subscription: CString,
}
impl TerminalView {
    fn input(&self, bytes: &[u8], generation: u64) -> i32 {
        unsafe {
            kodosi_terminal_input(
                self.handle,
                self.session.as_ptr(),
                self.incarnation.as_ptr(),
                self.subscription.as_ptr(),
                generation,
                bytes.as_ptr(),
                bytes.len(),
            )
        }
    }
    fn refresh(&self) -> i32 {
        unsafe {
            kodosi_terminal_refresh(
                self.handle,
                self.session.as_ptr(),
                self.subscription.as_ptr(),
                1,
            )
        }
    }
    fn disconnect(&self) -> i32 {
        unsafe {
            kodosi_terminal_disconnect(
                self.handle,
                self.session.as_ptr(),
                self.subscription.as_ptr(),
                1,
            )
        }
    }
}
fn start_runtime() -> (Box<Context>, mpsc::Receiver<Message>, Running) {
    let (tx, rx) = mpsc::channel();
    let mut context = Box::new(Context { sender: tx });
    let callbacks = KodosiCallbacks {
        on_event: Some(event),
        on_terminal_data: Some(output),
        on_terminal_control: Some(control),
        on_terminal_connect_result: Some(connected),
        on_terminal_checkpoint: Some(checkpoint),
    };
    let handle = unsafe {
        kodosi_start(
            &raw const callbacks,
            std::mem::size_of::<KodosiCallbacks>(),
            std::ptr::from_mut(&mut *context).cast(),
        )
    };
    assert!(!handle.is_null());
    (context, rx, Running(handle))
}

fn exercise_terminal() {
    let (context, rx, running) = start_runtime();
    let handle = running.0;
    receive_until(&rx, |message| {
        matches!(message, Message::Event(ref event) if event["type"] == "sessions.snapshot")
            .then_some(())
    });
    send(
        handle,
        json!({"type":"session.create","requestId":Uuid::now_v7(),"name":"FFI lifecycle test","workingDir":std::env::var("HOME").unwrap()}),
    );
    let (session, incarnation) = receive_until(&rx, |message| match message {
        Message::Event(event) if event["type"] == "sessions.snapshot" => {
            event["sessions"].as_array()?.first().map(|entry| {
                (
                    entry["id"].as_str().unwrap().to_owned(),
                    entry["incarnationId"].as_str().unwrap().to_owned(),
                )
            })
        }
        _ => None,
    });
    let view = TerminalView {
        handle,
        session: CString::new(session).unwrap(),
        incarnation: CString::new(incarnation).unwrap(),
        subscription: CString::new("ffi-test-view").unwrap(),
    };
    assert_eq!(
        unsafe {
            kodosi_terminal_connect(handle, view.session.as_ptr(), view.subscription.as_ptr(), 1)
        },
        KODOSI_FFI_OK
    );
    let mut next = receive_until(&rx, |message| match message {
        Message::Checkpoint(sequence) => Some(sequence),
        _ => None,
    });
    receive_until(&rx, |message| match message {
        Message::Connected(code) => {
            assert_eq!(code, KODOSI_FFI_OK);
            Some(())
        }
        _ => None,
    });

    let input = b"printf '\\106\\106\\111\\137\\114\\111\\126\\105\\137\\117\\113\\137\\065\\065\\060\\061\\012'\r";
    assert_eq!(view.input(input, 1), KODOSI_FFI_OK);
    let mut collected = Vec::new();
    receive_until(&rx, |message| match message {
        Message::Data(sequence, bytes) => {
            assert_eq!(sequence, next);
            next += 1;
            collected.extend(bytes);
            collected
                .windows(b"FFI_LIVE_OK_5501".len())
                .any(|bytes| bytes == b"FFI_LIVE_OK_5501")
                .then_some(())
        }
        _ => None,
    });
    assert_eq!(
        view.input(
            b"i=0; while [ $i -lt 20 ]; do printf 'cut-%d\\n' $i; i=$((i+1)); done\r",
            1
        ),
        KODOSI_FFI_OK
    );
    assert_eq!(view.refresh(), KODOSI_FFI_OK);
    receive_until(&rx, |message| match message {
        Message::Data(sequence, _) => {
            assert_eq!(sequence, next);
            next += 1;
            None
        }
        Message::Checkpoint(sequence) => {
            assert_eq!(sequence, next, "refresh skipped healthy-view continuation");
            Some(())
        }
        _ => None,
    });
    assert_eq!(view.input(input, 0), KODOSI_FFI_STALE_SUBSCRIPTION);
    assert_eq!(view.disconnect(), KODOSI_FFI_OK);
    assert_eq!(view.input(input, 1), KODOSI_FFI_STALE_SUBSCRIPTION);
    let key = handle as usize;
    let other = std::thread::spawn(move || unsafe { kodosi_stop(key as *mut c_void) });
    drop(running);
    other.join().unwrap();
    while rx.try_recv().is_ok() {}
    std::thread::sleep(Duration::from_millis(30));
    assert!(rx.try_recv().is_err());
    assert_eq!(
        unsafe { kodosi_send_command(handle, b"{}".as_ptr(), 2) },
        KODOSI_FFI_NULL_HANDLE
    );
    drop(context);
}
