//! The main thread: `AppKit`'s run loop, the notice in the menu bar while people watch, and the
//! pointer that viewers see.

use std::cell::{OnceCell, RefCell};

use dispatch2::DispatchQueue;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, NSObjectProtocol},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength, NSWorkspace, NSWorkspaceScreensDidSleepNotification,
    NSWorkspaceScreensDidWakeNotification, NSWorkspaceSessionDidBecomeActiveNotification,
    NSWorkspaceSessionDidResignActiveNotification,
};
use objc2_foundation::{NSDistributedNotificationCenter, NSString, ns_string};
use screen::{Notice, link::Source};
use tokio::sync::mpsc;

use super::{
    alert,
    hand::Tap,
    layout,
    pointer::Pointing,
    session::{self, Event},
};

thread_local! {
    static SHELL: OnceCell<Shell> = const { OnceCell::new() };
}

/// What the main thread keeps.
struct Shell {
    main: MainThreadMarker,
    stop: Retained<StopButton>,
    _changes: Retained<Changes>,
    notice: RefCell<Option<Retained<NSStatusItem>>>,
    pointer: RefCell<Pointing>,
    /// Hears the person at the computer while the screen is captured.
    hand: RefCell<Option<Tap>>,
}

pub(super) fn run(main: MainThreadMarker, link: String, token: String) {
    let app = NSApplication::sharedApplication(main);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let (say, saying) = mpsc::channel(session::MESSAGES);
    let (events, hearing) = mpsc::unbounded_channel();
    let shell = Shell {
        main,
        stop: StopButton::new(main, events.clone()),
        _changes: Changes::observe(main, events.clone()),
        notice: RefCell::new(None),
        pointer: RefCell::new(Pointing::new(say.clone())),
        hand: RefCell::new(None),
    };
    layout::learn();
    SHELL.with(|cell| {
        cell.get_or_init(|| shell);
    });
    let linked = std::thread::Builder::new()
        .name("kodosi-screen-link".to_owned())
        .spawn(move || {
            session::serve(&link, token, say, saying, events, hearing);
            DispatchQueue::main().exec_async(|| with(Shell::quit));
        });
    if linked.is_ok() {
        app.run();
    }
}

/// Shows who watches, or hides the notice once nobody does. Called from any thread.
pub(super) fn notice(notice: Option<Notice>) {
    DispatchQueue::main().exec_async(move || with(|shell| shell.notice(notice.as_ref())));
}

/// Follows the pointer over the shown source and hears the person at the computer, or stops.
/// Called from any thread.
pub(super) fn follow(source: Option<Source>) {
    DispatchQueue::main().exec_async(move || {
        with(|shell| {
            shell.pointer.borrow_mut().follow(source);
            shell.listen(source.is_some());
        });
    });
}

/// macOS stops a tap that it finds slow; it hears again at once.
pub(super) fn listen_again() {
    with(|shell| shell.listen(true));
}

pub(super) fn pointing(look: impl FnOnce(&mut Pointing)) {
    with(|shell| look(&mut shell.pointer.borrow_mut()));
}

fn with(work: impl FnOnce(&Shell)) {
    SHELL.with(|cell| {
        if let Some(shell) = cell.get() {
            work(shell);
        }
    });
}

impl Shell {
    fn notice(&self, notice: Option<&Notice>) {
        let bar = NSStatusBar::systemStatusBar();
        let mut shown = self.notice.borrow_mut();
        let Some(notice) = notice else {
            if let Some(item) = shown.take() {
                bar.removeStatusItem(&item);
            }
            return;
        };
        let item =
            shown.get_or_insert_with(|| bar.statusItemWithLength(NSVariableStatusItemLength));
        let watching = NSString::from_str(&notice.watching);
        if let Some(button) = item.button(self.main) {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                ns_string!("eye"),
                Some(&watching),
            );
            if let Some(image) = image {
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
            button.setToolTip(Some(&watching));
        }
        let menu = NSMenu::new(self.main);
        menu.setAutoenablesItems(false);
        let who = NSMenuItem::new(self.main);
        who.setTitle(&watching);
        who.setEnabled(false);
        menu.addItem(&who);
        menu.addItem(&NSMenuItem::separatorItem(self.main));
        // SAFETY: `stop:` is the stop button's action, and the button lives as long as the app.
        let stop = unsafe {
            let stop = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.main),
                &NSString::from_str(&notice.stop),
                Some(sel!(stop:)),
                ns_string!(""),
            );
            stop.setTarget(Some(&self.stop));
            stop
        };
        menu.addItem(&stop);
        item.setMenu(Some(&menu));
    }

    fn listen(&self, on: bool) {
        let mut hand = self.hand.borrow_mut();
        if on && hand.is_none() {
            *hand = Tap::new();
        }
        if let Some(tap) = hand.as_ref() {
            tap.enable(on);
        }
    }

    fn quit(&self) {
        if !alert::quit_after() {
            NSApplication::sharedApplication(self.main).terminate(None);
        }
    }
}

define_class!(
    /// The target of the notice's stop item.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KodosiScreenStop"]
    #[ivars = mpsc::UnboundedSender<Event>]
    struct StopButton;

    unsafe impl NSObjectProtocol for StopButton {}

    impl StopButton {
        #[unsafe(method(stop:))]
        fn stop(&self, _sender: Option<&AnyObject>) {
            drop(self.ivars().send(Event::Stopped));
            with(|shell| shell.notice(None));
        }
    }
);

impl StopButton {
    fn new(main: MainThreadMarker, events: mpsc::UnboundedSender<Event>) -> Retained<Self> {
        let this = Self::alloc(main).set_ivars(events);
        // SAFETY: `NSObject`'s `init` takes nothing.
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    /// Hears when the screen locks or unlocks, the displays sleep or wake, or the account on
    /// the display changes, so that viewers learn it at once.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KodosiScreenChanges"]
    #[ivars = mpsc::UnboundedSender<Event>]
    struct Changes;

    unsafe impl NSObjectProtocol for Changes {}

    impl Changes {
        #[unsafe(method(changed:))]
        fn changed(&self, _notification: Option<&AnyObject>) {
            drop(self.ivars().send(Event::Changed));
        }

        #[unsafe(method(layoutChanged:))]
        fn layout_changed(&self, _notification: Option<&AnyObject>) {
            layout::learn();
        }
    }
);

impl Changes {
    fn observe(main: MainThreadMarker, events: mpsc::UnboundedSender<Event>) -> Retained<Self> {
        let this = Self::alloc(main).set_ivars(events);
        // SAFETY: `NSObject`'s `init` takes nothing.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let distributed = NSDistributedNotificationCenter::defaultCenter();
        for name in [
            ns_string!("com.apple.screenIsLocked"),
            ns_string!("com.apple.screenIsUnlocked"),
        ] {
            // SAFETY: `changed:` takes the notification, and the observer lives as long as the
            // app.
            unsafe {
                distributed.addObserver_selector_name_object(
                    &this,
                    sel!(changed:),
                    Some(name),
                    None,
                );
            }
        }
        // SAFETY: as above.
        unsafe {
            distributed.addObserver_selector_name_object(
                &this,
                sel!(layoutChanged:),
                Some(ns_string!(
                    "com.apple.Carbon.TISNotifySelectedKeyboardInputSourceChanged"
                )),
                None,
            );
        }
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: AppKit's notification names, which live as long as the app.
        let names = unsafe {
            [
                NSWorkspaceScreensDidSleepNotification,
                NSWorkspaceScreensDidWakeNotification,
                NSWorkspaceSessionDidBecomeActiveNotification,
                NSWorkspaceSessionDidResignActiveNotification,
            ]
        };
        for name in names {
            // SAFETY: as above.
            unsafe {
                workspace.addObserver_selector_name_object(&this, sel!(changed:), Some(name), None);
            }
        }
        this
    }
}
