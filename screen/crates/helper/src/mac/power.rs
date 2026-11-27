//! Whether the screen can be seen: the lock, the account on the display and the display's sleep,
//! and the power assertions that wake the display and keep it awake while it is captured.

use objc2_core_foundation::{CFBoolean, CFString};
use objc2_core_graphics::{CGDisplayIsAsleep, CGMainDisplayID, CGSessionCopyCurrentDictionary};
use screen::link::Away;

use super::value;

type AssertionId = u32;

const SUCCESS: i32 = 0;
const ON: u32 = 255;
/// Someone at the computer, which turns the display on; a remote user would leave it off.
const USER_ACTIVE_LOCAL: u32 = 0;
const ASSERTION_NAME: &str = "Kodosi shows this screen";

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        kind: &CFString,
        level: u32,
        name: &CFString,
        id: *mut AssertionId,
    ) -> i32;
    fn IOPMAssertionDeclareUserActivity(name: &CFString, user: u32, id: *mut AssertionId) -> i32;
    fn IOPMAssertionRelease(id: AssertionId) -> i32;
}

/// Why this account's session shows nothing: it is locked, or another account has the display.
pub(super) fn session_away() -> Option<Away> {
    let session = CGSessionCopyCurrentDictionary()?;
    let says = |key: &'static str| {
        value::<CFBoolean>(&session, &CFString::from_static_str(key))
            .is_some_and(CFBoolean::as_bool)
    };
    if !says("kCGSSessionOnConsoleKey") {
        Some(Away::AnotherAccount)
    } else if says("CGSSessionScreenIsLocked") {
        Some(Away::Locked)
    } else {
        None
    }
}

pub(super) fn display_asleep() -> bool {
    CGDisplayIsAsleep(CGMainDisplayID())
}

/// The helper's power assertions, which end with it.
#[derive(Default)]
pub(super) struct Power {
    /// Keeps the display from sleeping while it is captured.
    awake: Option<AssertionId>,
    /// The last time the helper turned the display on.
    woken: Option<AssertionId>,
}

impl Power {
    /// Turns the display on, as a touch of the computer's mouse would.
    pub(super) fn wake(&mut self) {
        release(self.woken.take());
        let mut id = 0;
        // SAFETY: the name is a valid string and `id` is valid to write.
        let declared = unsafe {
            IOPMAssertionDeclareUserActivity(
                &CFString::from_static_str(ASSERTION_NAME),
                USER_ACTIVE_LOCAL,
                &raw mut id,
            )
        };
        self.woken = (declared == SUCCESS).then_some(id);
    }

    pub(super) fn keep_awake(&mut self, awake: bool) {
        if !awake {
            release(self.awake.take());
            return;
        }
        if self.awake.is_some() {
            return;
        }
        let mut id = 0;
        // SAFETY: the type and the name are valid strings and `id` is valid to write.
        let created = unsafe {
            IOPMAssertionCreateWithName(
                &CFString::from_static_str("PreventUserIdleDisplaySleep"),
                ON,
                &CFString::from_static_str(ASSERTION_NAME),
                &raw mut id,
            )
        };
        self.awake = (created == SUCCESS).then_some(id);
    }
}

impl Drop for Power {
    fn drop(&mut self) {
        release(self.awake.take());
        release(self.woken.take());
    }
}

fn release(id: Option<AssertionId>) {
    if let Some(id) = id {
        // SAFETY: the helper made the assertion and releases it once.
        unsafe { IOPMAssertionRelease(id) };
    }
}
