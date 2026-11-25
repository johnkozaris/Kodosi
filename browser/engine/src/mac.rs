use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, define_class, extern_methods, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObjectProtocol},
};
use objc2_app_kit::{NSApp, NSApplication, NSEvent};
use std::cell::Cell;

const FRAMEWORK: &str = "Chromium Embedded Framework.framework/Chromium Embedded Framework";

#[derive(Default)]
pub(crate) struct Ivars {
    handling_send_event: Cell<Bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = Ivars]
    pub(crate) struct EngineApplication;

    impl EngineApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            let was = self.is_handling_send_event();
            if !was {
                self.set_handling_send_event(true);
            }
            let _: () = msg_send![super(self), sendEvent: event];
            if !was {
                self.set_handling_send_event(false);
            }
        }

        #[unsafe(method(terminate:))]
        unsafe fn terminate(&self, _sender: &AnyObject) {
            cef::quit_message_loop();
        }
    }

    unsafe impl CrAppControlProtocol for EngineApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn _set_handling_send_event(&self, handling: Bool) {
            self.ivars().handling_send_event.set(handling);
        }
    }

    unsafe impl CrAppProtocol for EngineApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn _is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CefAppProtocol for EngineApplication {}
);

impl EngineApplication {
    extern_methods! {
        #[unsafe(method(sharedApplication))]
        fn shared_application() -> Retained<Self>;

        #[unsafe(method(setHandlingSendEvent:))]
        fn set_handling_send_event(&self, handling: bool);

        #[unsafe(method(isHandlingSendEvent))]
        fn is_handling_send_event(&self) -> bool;
    }
}

/// Registers `EngineApplication` as `NSApp`. Must be called on the main thread before anything
/// else initializes the shared application.
pub(crate) fn install_application() -> bool {
    let _ = EngineApplication::shared_application();
    MainThreadMarker::new()
        .is_some_and(|main| NSApp(main).isKindOfClass(EngineApplication::class()))
}

/// Pre-checks that the CEF framework exists in the bundle because `cef::library_loader` panics
/// when it is missing.
pub(crate) fn load_framework(helper: bool) -> Option<cef::library_loader::LibraryLoader> {
    let program = std::env::current_exe().ok()?;
    let frameworks = program
        .parent()?
        .join(if helper { "../../.." } else { "../Frameworks" });
    std::fs::exists(frameworks.join(FRAMEWORK))
        .ok()?
        .then(|| cef::library_loader::LibraryLoader::new(&program, helper))
        .filter(cef::library_loader::LibraryLoader::load)
}
