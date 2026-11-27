//! Kodosi Screen's own alert, in which the person at the Mac allows an account or does not.
//!
//! Only this program's click counts, so a look-alike alert of another program gains nothing. A
//! copy of the bundle with a changed file in its resources still runs with the team's signature,
//! so the alert takes its words from the host and nothing from the bundle but the icon, and shows
//! only while the bundle on the disk is whole.

use std::{cell::Cell, ptr, ptr::NonNull};

use block2::RcBlock;
use dispatch2::MainThreadBound;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, define_class, msg_send, rc::Retained,
    runtime::NSObject, sel,
};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSApplication, NSEvent,
    NSEventMask,
};
use objc2_foundation::{
    NSObjectNSThreadPerformAdditions as _, NSRunLoop, NSRunLoopCommonModes, NSString, NSTimer,
};
use objc2_security::{SecCSFlags, SecCode, errSecSuccess, kSecCSStrictValidate};
use screen::link::Question;
use tokio::sync::mpsc;

use super::session::Event;

/// How long "Allow" stays off after the alert shows, so that a click or a key meant for another
/// program cannot allow.
const WAKING: f64 = 1.0;
const ESCAPE: u16 = 53;

thread_local! {
    static SHOWING: Cell<Showing> = const { Cell::new(Showing::No) };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Showing {
    No,
    Yes,
    /// The helper quits once the alert is gone.
    Quitting,
}

/// Whether this program's bundle on the disk is as its team signed it, resources included.
pub(super) fn intact() -> Result<(), String> {
    let mut code = ptr::null_mut();
    // SAFETY: `code` is valid to write.
    let found = unsafe { SecCode::copy_self(SecCSFlags::DefaultFlags, NonNull::from(&mut code)) };
    let code = NonNull::new(code)
        .filter(|_| found == errSecSuccess)
        // SAFETY: the call succeeded, so `code` holds a retained code object.
        .map(|code| unsafe { objc2_core_foundation::CFRetained::from_raw(code) })
        .ok_or("Kodosi Screen cannot read its own signature")?;
    let mut on_disk = ptr::null();
    // SAFETY: `on_disk` is valid to write.
    let read =
        unsafe { code.copy_static_code(SecCSFlags::DefaultFlags, NonNull::from(&mut on_disk)) };
    let on_disk = NonNull::new(on_disk.cast_mut())
        .filter(|_| read == errSecSuccess)
        // SAFETY: the call succeeded, so `on_disk` holds a retained code object.
        .map(|on_disk| unsafe { objc2_core_foundation::CFRetained::from_raw(on_disk) })
        .ok_or("Kodosi Screen cannot find its bundle")?;
    // SAFETY: `on_disk` is a valid code object.
    let valid = unsafe { on_disk.check_validity(SecCSFlags(kSecCSStrictValidate), None) };
    if valid == errSecSuccess {
        Ok(())
    } else {
        Err(format!(
            "Kodosi Screen's bundle is not as its team signed it ({valid}), so it asks nothing"
        ))
    }
}

/// Shows alert number `alert` on the main thread, which answers through `events`.
pub(super) fn show(question: Question, alert: u64, events: mpsc::UnboundedSender<Event>) {
    let asking = Asking::new(Ask {
        question,
        alert,
        events,
    });
    // SAFETY: `show` takes nothing. Not a block of the main dispatch queue, so that the queue
    // still runs while the alert shows, and the helper can quit then.
    unsafe {
        asking.performSelectorOnMainThread_withObject_waitUntilDone(sel!(show), None, false);
    }
}

/// Ends the alert that shows, and returns whether one did, after which the helper quits.
pub(super) fn quit_after() -> bool {
    let showing = SHOWING.get() != Showing::No;
    if showing && let Some(main) = MainThreadMarker::new() {
        SHOWING.set(Showing::Quitting);
        NSApplication::sharedApplication(main).abortModal();
    }
    showing
}

struct Ask {
    question: Question,
    alert: u64,
    events: mpsc::UnboundedSender<Event>,
}

define_class!(
    /// An alert that the link's thread asks for, which shows on the main thread.
    #[unsafe(super(NSObject))]
    #[name = "KodosiScreenAsking"]
    #[ivars = Ask]
    struct Asking;

    impl Asking {
        #[unsafe(method(show))]
        fn show(&self) {
            let Some(main) = MainThreadMarker::new() else {
                return;
            };
            let ask = self.ivars();
            let allowed = ask_in(main, &ask.question);
            drop(ask.events.send(Event::Answered {
                alert: ask.alert,
                answer: Ok(allowed),
            }));
            if SHOWING.replace(Showing::No) == Showing::Quitting {
                NSApplication::sharedApplication(main).terminate(None);
            }
        }
    }
);

impl Asking {
    fn new(ask: Ask) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ask);
        // SAFETY: `NSObject`'s `init` takes nothing.
        unsafe { msg_send![super(this), init] }
    }
}

/// "Don't Allow" is the default and takes Return and Escape.
fn ask_in(main: MainThreadMarker, question: &Question) -> bool {
    let app = NSApplication::sharedApplication(main);
    let alert = NSAlert::new(main);
    alert.setMessageText(&NSString::from_str(&question.message));
    alert.setInformativeText(&NSString::from_str(&question.detail));
    alert.addButtonWithTitle(&NSString::from_str(&question.refuse));
    let allow = alert.addButtonWithTitle(&NSString::from_str(&question.allow));
    allow.setEnabled(false);
    let allow = MainThreadBound::new(allow, main);
    let waking = RcBlock::new(move |_: NonNull<NSTimer>| {
        if let Some(main) = MainThreadMarker::new() {
            allow.get(main).setEnabled(true);
        }
    });
    // SAFETY: the timer runs on the main run loop only, where the block touches the button.
    let timer = unsafe { NSTimer::timerWithTimeInterval_repeats_block(WAKING, false, &waking) };
    // SAFETY: the main run loop on the main thread, in every mode, the alert's included.
    unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
    let escape = RcBlock::new(|event: NonNull<NSEvent>| {
        // SAFETY: AppKit hands the monitor a valid event.
        if unsafe { event.as_ref() }.keyCode() == ESCAPE
            && let Some(main) = MainThreadMarker::new()
        {
            NSApplication::sharedApplication(main).stopModalWithCode(NSAlertFirstButtonReturn);
            return ptr::null_mut();
        }
        event.as_ptr()
    });
    // SAFETY: the block takes and returns events, and the monitor goes before the block.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &escape)
    };
    app.activate();
    SHOWING.set(Showing::Yes);
    let answer = alert.runModal();
    timer.invalidate();
    if let Some(monitor) = monitor {
        // SAFETY: the monitor that this function added.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
    answer == NSAlertSecondButtonReturn
}
