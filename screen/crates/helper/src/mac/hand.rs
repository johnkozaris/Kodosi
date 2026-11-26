//! The person at the computer comes first: while they use its own mouse or keyboard, viewers'
//! input waits a moment, so that two hands never fight over the pointer.

use std::{
    ffi::c_void,
    ptr::{self, NonNull},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use objc2_core_foundation::{
    CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

use super::app;

/// How long the person's own input keeps viewers' input away.
const FIRST: Duration = Duration::from_secs(1);
const HEARD: [CGEventType; 14] = [
    CGEventType::MouseMoved,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::LeftMouseDragged,
    CGEventType::RightMouseDown,
    CGEventType::RightMouseUp,
    CGEventType::RightMouseDragged,
    CGEventType::OtherMouseDown,
    CGEventType::OtherMouseUp,
    CGEventType::OtherMouseDragged,
    CGEventType::ScrollWheel,
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::FlagsChanged,
];

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

/// Hears the computer's input on the main thread's run loop, without changing it.
pub(super) struct Tap {
    port: CFRetained<CFMachPort>,
    _source: CFRetained<CFRunLoopSource>,
}

impl Tap {
    /// `None` until macOS lets Kodosi Screen control the computer.
    pub(super) fn new() -> Option<Self> {
        let mask = HEARD
            .iter()
            .fold(0, |mask: CGEventMask, kind| mask | 1 << kind.0);
        // SAFETY: the callback only reads the events that it is given, and takes no user data.
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::HIDEventTap,
                CGEventTapPlacement::TailAppendEventTap,
                CGEventTapOptions::ListenOnly,
                mask,
                Some(heard),
                ptr::null_mut(),
            )
        }?;
        let source = CFMachPort::new_run_loop_source(None, Some(&port), 0)?;
        // SAFETY: a Core Foundation constant.
        let modes = unsafe { kCFRunLoopCommonModes };
        CFRunLoop::main()?.add_source(Some(&source), modes);
        Some(Self {
            port,
            _source: source,
        })
    }

    pub(super) fn enable(&self, on: bool) {
        CGEvent::tap_enable(&self.port, on);
    }
}

/// Marks the input that the helper posts, so that the tap tells it from the person's.
pub(super) const MARK: i64 = 0x4b6f_646f_7369;

/// Input without the helper's mark is the person's.
unsafe extern "C-unwind" fn heard(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    _user: *mut c_void,
) -> *mut CGEvent {
    if matches!(
        kind,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        app::listen_again();
        return event.as_ptr();
    }
    // SAFETY: the event is valid during the call.
    let event_ref = unsafe { event.as_ref() };
    if CGEvent::integer_value_field(Some(event_ref), CGEventField::EventSourceUserData) != MARK {
        LOCAL.store(now().max(1), Ordering::Relaxed);
    }
    event.as_ptr()
}
