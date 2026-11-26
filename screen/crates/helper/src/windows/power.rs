//! Whether the screen can be seen: the account on the display, the lock and the display's sleep,
//! and the request that keeps the display awake while it is captured.

use std::sync::atomic::{AtomicU32, Ordering};

use screen::link::Away;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Power::{
                PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired, PowerSetRequest,
            },
            RemoteDesktop::{
                ProcessIdToSessionId, WTS_CURRENT_SERVER_HANDLE, WTSActive, WTSFreeMemory,
                WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
            },
            StationsAndDesktops::{
                CloseDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS,
                GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
            },
            SystemServices::POWER_REQUEST_CONTEXT_VERSION,
            Threading::{
                GetCurrentProcessId, POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT,
                REASON_CONTEXT_0,
            },
        },
        UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
        },
    },
    core::{PWSTR, w},
};

/// `GUID_CONSOLE_DISPLAY_STATE`'s values.
const DISPLAY_OFF: u32 = 0;
const DISPLAY_ON: u32 = 1;
/// The lock in `WTSINFOEX_LEVEL1_W::SessionFlags`.
const SESSION_LOCKED: i32 = 0;

/// The display's state as Windows last told the helper's window.
static DISPLAY: AtomicU32 = AtomicU32::new(DISPLAY_ON);

pub(super) fn display_changed(state: u32) {
    DISPLAY.store(state, Ordering::Relaxed);
}

pub(super) fn display_asleep() -> bool {
    DISPLAY.load(Ordering::Relaxed) == DISPLAY_OFF
}

/// Why this account's session shows nothing: another account has the display, or the session is
/// locked, or Windows shows another desktop, as for an administrator's prompt.
pub(super) fn session_away() -> Option<Away> {
    let mut session = 0;
    // SAFETY: `session` is valid to write.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &raw mut session) }.ok()?;
    let mut buffer = PWSTR::null();
    let mut bytes = 0;
    // SAFETY: both pointers are valid to write, and Windows' buffer is freed below.
    unsafe {
        WTSQuerySessionInformationW(
            Some(WTS_CURRENT_SERVER_HANDLE),
            session,
            WTSSessionInfoEx,
            &raw mut buffer,
            &raw mut bytes,
        )
    }
    .ok()?;
    let fits = usize::try_from(bytes).is_ok_and(|bytes| bytes >= size_of::<WTSINFOEXW>());
    // SAFETY: Windows wrote a `WTSINFOEXW` of level 1 into the buffer.
    let level = fits.then(|| unsafe {
        let level = buffer
            .0
            .cast::<WTSINFOEXW>()
            .read_unaligned()
            .Data
            .WTSInfoExLevel1;
        (level.SessionState, level.SessionFlags)
    });
    // SAFETY: the buffer that Windows gave above, freed once.
    unsafe { WTSFreeMemory(buffer.0.cast()) };
    let (state, flags) = level?;
    if state != WTSActive {
        Some(Away::AnotherAccount)
    } else if flags == SESSION_LOCKED || !shows_default_desktop() {
        Some(Away::Locked)
    } else {
        None
    }
}

/// True while the session's own desktop takes the input, rather than the lock screen's or an
/// administrator's prompt's, which only Windows itself may see.
fn shows_default_desktop() -> bool {
    // SAFETY: the desktop is closed below.
    let Ok(desktop) =
        (unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) })
    else {
        return false;
    };
    let mut name = [0_u16; 64];
    let mut needed = 0;
    // SAFETY: `name` holds as many bytes as the call is told, and `needed` is valid to write.
    let read = unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            u32::try_from(size_of_val(&name)).unwrap_or_default(),
            Some(&raw mut needed),
        )
    };
    // SAFETY: the desktop opened above.
    drop(unsafe { CloseDesktop(desktop) });
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    read.is_ok() && String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case("Default")
}

/// Turns the display on, as a touch of the computer's mouse would.
pub(super) fn wake() {
    let nudge = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: MOUSEEVENTF_MOVE,
                ..MOUSEINPUT::default()
            },
        },
    };
    // SAFETY: one complete input of the size given.
    unsafe {
        SendInput(
            &[nudge],
            i32::try_from(size_of::<INPUT>()).unwrap_or_default(),
        )
    };
}

/// The helper's request that keeps the display awake, which ends with it.
#[derive(Default)]
pub(super) struct Power {
    awake: Option<HANDLE>,
}

impl Power {
    pub(super) fn keep_awake(&mut self, awake: bool) {
        if !awake {
            release(self.awake.take());
            return;
        }
        if self.awake.is_some() {
            return;
        }
        let reason = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(
                    w!("Kodosi Screen shows this screen").as_ptr().cast_mut(),
                ),
            },
        };
        // SAFETY: the reason is complete, and Windows copies its text.
        let Ok(request) = (unsafe { PowerCreateRequest(&raw const reason) }) else {
            return;
        };
        // SAFETY: the request made above.
        if unsafe { PowerSetRequest(request, PowerRequestDisplayRequired) }.is_ok() {
            self.awake = Some(request);
        } else {
            // SAFETY: as above, closed once.
            drop(unsafe { CloseHandle(request) });
        }
    }
}

impl Drop for Power {
    fn drop(&mut self) {
        release(self.awake.take());
    }
}

fn release(request: Option<HANDLE>) {
    if let Some(request) = request {
        // SAFETY: the helper made the request and releases it once.
        unsafe {
            drop(PowerClearRequest(request, PowerRequestDisplayRequired));
            drop(CloseHandle(request));
        }
    }
}
