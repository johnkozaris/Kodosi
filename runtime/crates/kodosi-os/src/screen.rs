use zbus::{Connection, OwnedVariant};

use crate::bus;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const SCREEN_CAST: &str = "org.freedesktop.portal.ScreenCast";
const REMOTE_DESKTOP: &str = "org.freedesktop.portal.RemoteDesktop";
const MONITOR: u32 = 1;
const KEYBOARD_AND_POINTER: u32 = 1 | 2;

/// Whether the desktop's portal shares the displays and plays the keyboard and the pointer, as
/// Kodosi Screen needs on Wayland.
pub fn portal_shares() -> bool {
    bus::ask_session(async |bus| {
        let sources = offered(bus, SCREEN_CAST, "AvailableSourceTypes").await?;
        let devices = offered(bus, REMOTE_DESKTOP, "AvailableDeviceTypes").await?;
        Some(sources & MONITOR != 0 && devices & KEYBOARD_AND_POINTER == KEYBOARD_AND_POINTER)
    })
    .unwrap_or(false)
}

/// What the portal's `interface` says it offers, as the bits of `property`.
async fn offered(bus: &Connection, interface: &str, property: &str) -> Option<u32> {
    let reply = bus
        .call_method(
            Some(PORTAL),
            PORTAL_PATH,
            Some(PROPERTIES),
            "Get",
            &(interface, property),
        )
        .await
        .ok()?;
    u32::try_from(reply.body().deserialize::<OwnedVariant>().ok()?).ok()
}
