#![allow(unsafe_code)]

use std::{
    io,
    os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
    ptr,
};

use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE, LUID},
    Security::{
        AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
        SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    },
    System::{
        Power::{
            GetPwrCapabilities, GetSystemPowerStatus, IsPwrSuspendAllowed, PowerClearRequest,
            PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
            SYSTEM_POWER_CAPABILITIES, SYSTEM_POWER_STATUS, SetSuspendState,
        },
        Threading::{
            GetCurrentProcess, OpenProcessToken, POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            REASON_CONTEXT, REASON_CONTEXT_0,
        },
    },
    UI::WindowsAndMessaging::{
        HWND_BROADCAST, SC_MONITORPOWER, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SYSCOMMAND,
    },
};

use crate::win;

const REQUEST_CONTEXT_VERSION: u32 = 0;
const MONITOR_OFF: isize = 2;
const MESSAGE_WAIT_MS: u32 = 2000;

pub(super) struct Awake(OwnedHandle);

impl Awake {
    pub(super) fn hold(reason: &str) -> io::Result<Self> {
        let mut reason = win::wide(reason);
        let context = REASON_CONTEXT {
            Version: REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: reason.as_mut_ptr(),
            },
        };
        // SAFETY: the context and its string live through the call, which copies them.
        let request = unsafe { PowerCreateRequest(&raw const context) };
        if request.is_null() || request == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the call above made this handle, and nothing else owns it.
        let request = unsafe { OwnedHandle::from_raw_handle(request) };
        // SAFETY: the handle is an open power request.
        if unsafe { PowerSetRequest(request.as_raw_handle(), PowerRequestSystemRequired) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(request))
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        // SAFETY: the handle is the open power request that `hold` set.
        unsafe { PowerClearRequest(self.0.as_raw_handle(), PowerRequestSystemRequired) };
    }
}

/// A computer with Modern Standby has no S3 sleep. It goes to standby when its screen turns off.
fn sleeps_with_its_screen() -> bool {
    let mut capabilities = SYSTEM_POWER_CAPABILITIES::default();
    // SAFETY: `capabilities` is a valid place for the result.
    unsafe { GetPwrCapabilities(&raw mut capabilities) && capabilities.AoAc }
}

pub(super) fn on_battery() -> bool {
    const AC_OFFLINE: u8 = 0;
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is a valid place for the result.
    unsafe { GetSystemPowerStatus(&raw mut status) != 0 && status.ACLineStatus == AC_OFFLINE }
}

pub(super) fn can_sleep() -> io::Result<()> {
    // SAFETY: the call takes no arguments and only reads the power policy.
    if unsafe { IsPwrSuspendAllowed() } || sleeps_with_its_screen() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows has no sleep state on this computer",
        ))
    }
}

pub(super) fn sleep() -> io::Result<super::Sleep> {
    // SAFETY: the call takes no arguments and only reads the power policy.
    if unsafe { IsPwrSuspendAllowed() } {
        allow_shutdown()?;
        // SAFETY: the call takes no pointers. Wake events stay on, so that the network card can
        // wake the computer.
        if unsafe { SetSuspendState(false, false, false) } {
            Ok(super::Sleep::Ended)
        } else {
            Err(io::Error::last_os_error())
        }
    } else if sleeps_with_its_screen() {
        let mut result = 0;
        // SAFETY: `result` is a valid place for the answer, and the message carries no pointers.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SYSCOMMAND,
                SC_MONITORPOWER as usize,
                MONITOR_OFF,
                SMTO_ABORTIFHUNG,
                MESSAGE_WAIT_MS,
                &raw mut result,
            );
        }
        Ok(super::Sleep::Starts)
    } else {
        can_sleep().map(|()| super::Sleep::Starts)
    }
}

/// `SetSuspendState` needs the shutdown privilege, which each account has but no process starts
/// with.
fn allow_shutdown() -> io::Result<()> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: `token` is a valid place for the handle of this process's own token.
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &raw mut token,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call above made this handle, and nothing else owns it.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut privilege = LUID::default();
    // SAFETY: the name is a constant string, and `privilege` is a valid place for the result.
    if unsafe { LookupPrivilegeValueW(ptr::null(), SE_SHUTDOWN_NAME, &raw mut privilege) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let wanted = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: privilege,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: `wanted` lives through the call, and the earlier state is not asked for.
    if unsafe {
        AdjustTokenPrivileges(
            token.as_raw_handle(),
            0,
            &raw const wanted,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
