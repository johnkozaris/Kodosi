//! The person at the computer comes first: while they use its own mouse or keyboard, viewers'
//! input waits a moment, so that two hands never fight over the pointer.

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED,
        MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_QUIT,
    },
};

/// How long the person's own input keeps viewers' input away.
const FIRST: Duration = Duration::from_secs(1);

static STARTED: OnceLock<Instant> = OnceLock::new();
/// When the person last used the computer, in milliseconds after `STARTED`, or 0 for never.
static LOCAL: AtomicU64 = AtomicU64::new(0);

/// True while the person at the computer uses it or just did.
pub(super) fn busy() -> bool {
    let at = LOCAL.load(Ordering::Relaxed);
    at != 0 && Duration::from_millis(now().saturating_sub(at)) < FIRST
}

fn now() -> u64 {
    u64::try_from(STARTED.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Hears the computer's own mouse and keyboard on a thread of its own, while the screen is
/// captured. Windows waits for these hooks before it moves the pointer or types, so they only
/// note the time and return.
pub(super) struct Listening {
    thread: u32,
    listener: Option<JoinHandle<()>>,
}

impl Listening {
    pub(super) fn start() -> Option<Self> {
        let (ready, started) = mpsc::channel();
        let listener = std::thread::Builder::new()
            .name("kodosi-screen-hand".to_owned())
            .spawn(move || listen(&ready))
            .ok()?;
        let thread = started.recv().ok().flatten();
        let listening = Self {
            thread: thread.unwrap_or_default(),
            listener: Some(listener),
        };
        thread.map(|_| listening)
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        // SAFETY: a message to the listening thread's queue.
        drop(unsafe { PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0)) });
        if let Some(listener) = self.listener.take() {
            drop(listener.join());
        }
    }
}

fn listen(ready: &mpsc::Sender<Option<u32>>) {
    let mut message = MSG::default();
    // SAFETY: `message` is valid to write; peeking makes the thread's queue, which the stop
    // message needs.
    let _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
    // SAFETY: the helper's own module, whose procedures only read what they are given; the hooks
    // end with the thread.
    let hooks = unsafe {
        let module = GetModuleHandleW(None).ok().map(HINSTANCE::from);
        (
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), module, 0),
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module, 0),
        )
    };
    let (Ok(mouse), Ok(keyboard)) = hooks else {
        let _ = ready.send(None);
        return;
    };
    // SAFETY: a getter.
    let _ = ready.send(Some(unsafe { GetCurrentThreadId() }));
    // SAFETY: `message` is valid to write; the hooks' procedures run inside.
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {}
    // SAFETY: the hooks above.
    unsafe {
        drop(UnhookWindowsHookEx(mouse));
        drop(UnhookWindowsHookEx(keyboard));
    }
}

/// Input that a program sent, such as the helper itself, is not the person's.
unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == i32::try_from(HC_ACTION).unwrap_or_default() {
        // SAFETY: a mouse hook's action comes with the mouse's input.
        let input = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if input.flags & LLMHF_INJECTED == 0 {
            LOCAL.store(now().max(1), Ordering::Relaxed);
        }
    }
    // SAFETY: passes on what the hook was given.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == i32::try_from(HC_ACTION).unwrap_or_default() {
        // SAFETY: a keyboard hook's action comes with the key's input.
        let input = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if input.flags.0 & LLKHF_INJECTED.0 == 0 {
            LOCAL.store(now().max(1), Ordering::Relaxed);
        }
    }
    // SAFETY: passes on what the hook was given.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
