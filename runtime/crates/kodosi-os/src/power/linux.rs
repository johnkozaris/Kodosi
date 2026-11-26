use std::{collections::HashMap, io, os::fd::OwnedFd};

use zbus::{
    blocking::Connection,
    zvariant::{self, OwnedObjectPath, Value},
};

use crate::bus;

const LOGIN: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const LOGIN_MANAGER: &str = "org.freedesktop.login1.Manager";
const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SUSPEND: u32 = 4;
const POWER_SUPPLIES: &str = "/sys/class/power_supply";

/// logind's `idle` lock stops only the sleep that logind itself starts. A desktop starts its own
/// sleep after idle time and reads only the inhibitors of its session, which the portal sets. A
/// logind `sleep` lock is not used: since systemd 257 it also refuses the person's own Suspend.
pub(super) struct Awake {
    _idle: Option<OwnedFd>,
    _desktop: Option<DesktopHold>,
}

struct DesktopHold {
    bus: Connection,
    request: OwnedObjectPath,
}

impl Drop for DesktopHold {
    fn drop(&mut self) {
        drop(self.bus.call_method(
            Some(PORTAL),
            &self.request,
            Some("org.freedesktop.portal.Request"),
            "Close",
            &(),
        ));
    }
}

impl Awake {
    pub(super) fn hold(reason: &str) -> io::Result<Self> {
        let idle = idle_lock(reason);
        let desktop = desktop_hold(reason);
        if idle.is_none() && desktop.is_none() {
            return Err(io::Error::other(
                "neither logind nor the desktop took the request to stay awake",
            ));
        }
        Ok(Self {
            _idle: idle,
            _desktop: desktop,
        })
    }
}

fn idle_lock(reason: &str) -> Option<OwnedFd> {
    let reason = reason.to_owned();
    bus::ask_system(move |bus| {
        let lock: zvariant::OwnedFd = bus
            .call_method(
                Some(LOGIN),
                LOGIN_PATH,
                Some(LOGIN_MANAGER),
                "Inhibit",
                &("idle", "Kodosi", reason.as_str(), "block"),
            )
            .ok()?
            .body()
            .deserialize()
            .ok()?;
        Some(OwnedFd::from(lock))
    })
}

fn desktop_hold(reason: &str) -> Option<DesktopHold> {
    let reason = reason.to_owned();
    bus::ask_session(move |bus| {
        let options = HashMap::from([("reason", Value::from(reason.as_str()))]);
        let request: OwnedObjectPath = bus
            .call_method(
                Some(PORTAL),
                PORTAL_PATH,
                Some("org.freedesktop.portal.Inhibit"),
                "Inhibit",
                &("", PORTAL_SUSPEND, options),
            )
            .ok()?
            .body()
            .deserialize()
            .ok()?;
        Some(DesktopHold {
            bus: bus.clone(),
            request,
        })
    })
}

/// The battery of a mouse or a headset has the scope `Device`, and it does not power the computer.
pub(super) const fn sleeps_with_its_screen() -> bool {
    false
}

pub(super) fn on_battery() -> bool {
    let Ok(supplies) = std::fs::read_dir(POWER_SUPPLIES) else {
        return false;
    };
    let mut discharging = false;
    for supply in supplies.flatten() {
        let read =
            |name: &str| std::fs::read_to_string(supply.path().join(name)).unwrap_or_default();
        match read("type").trim() {
            "Mains" | "USB" if read("online").trim() == "1" => return false,
            "Battery"
                if read("scope").trim() != "Device" && read("status").trim() == "Discharging" =>
            {
                discharging = true;
            }
            _ => {}
        }
    }
    discharging
}

pub(super) fn can_sleep() -> io::Result<()> {
    let answer: Option<String> = bus::ask_system(|bus| {
        bus.call_method(
            Some(LOGIN),
            LOGIN_PATH,
            Some(LOGIN_MANAGER),
            "CanSuspend",
            &(),
        )
        .ok()?
        .body()
        .deserialize()
        .ok()
    });
    match answer.as_deref() {
        Some("yes") => Ok(()),
        Some(answer) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("logind answers `{answer}` to a suspend that no person confirms"),
        )),
        None => Err(io::Error::other("logind did not answer")),
    }
}

pub(super) fn sleep() -> io::Result<super::Sleep> {
    bus::ask_system(|bus| {
        Some(
            bus.call_method(
                Some(LOGIN),
                LOGIN_PATH,
                Some(LOGIN_MANAGER),
                "Suspend",
                &(false,),
            )
            .map(drop)
            .map_err(|error| error.to_string()),
        )
    })
    .ok_or_else(|| io::Error::other("logind did not answer"))?
    .map(|()| super::Sleep::Starts)
    .map_err(io::Error::other)
}
