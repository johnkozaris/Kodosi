#![allow(unsafe_code)]

use std::{io, process::Command};

use core_foundation::{
    base::TCFType as _,
    string::{CFString, CFStringRef},
};

const LEVEL_ON: u32 = 255;
const SUCCESS: i32 = 0;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        kind: CFStringRef,
        level: u32,
        name: CFStringRef,
        assertion: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(assertion: u32) -> i32;
}

pub(super) struct Awake(u32);

impl Awake {
    pub(super) fn hold(reason: &str) -> io::Result<Self> {
        let kind = CFString::from_static_string("PreventUserIdleSystemSleep");
        let name = CFString::new(reason);
        let mut assertion = 0;
        // SAFETY: both strings live through the call, and `assertion` is a valid place for the
        // result.
        let status = unsafe {
            IOPMAssertionCreateWithName(
                kind.as_concrete_TypeRef(),
                LEVEL_ON,
                name.as_concrete_TypeRef(),
                &raw mut assertion,
            )
        };
        if status == SUCCESS {
            Ok(Self(assertion))
        } else {
            Err(io::Error::other(format!(
                "macOS refused the request to stay awake ({status:#x})"
            )))
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        // SAFETY: the assertion is the one that `hold` made, and it is released once.
        unsafe { IOPMAssertionRelease(self.0) };
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the other systems can refuse to sleep"
)]
pub(super) const fn can_sleep() -> io::Result<()> {
    Ok(())
}

pub(super) fn sleep() -> io::Result<super::Sleep> {
    let output = Command::new("/usr/bin/pmset").arg("sleepnow").output()?;
    if output.status.success() {
        Ok(super::Sleep::Starts)
    } else {
        Err(io::Error::other(format!(
            "pmset did not start the sleep: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
