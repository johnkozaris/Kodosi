#![allow(unsafe_code)]

use std::{collections::BTreeMap, ffi::c_void, net::Ipv4Addr, process::Command};

use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::TCFType as _,
    string::{CFString, CFStringRef},
};

use super::{Hardware, Link, Medium, Wakes, hardware, unix};

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCNetworkInterfaceCopyAll() -> CFArrayRef;
    fn SCNetworkInterfaceGetBSDName(interface: *const c_void) -> CFStringRef;
    fn SCNetworkInterfaceGetInterfaceType(interface: *const c_void) -> CFStringRef;
    fn SCNetworkInterfaceGetHardwareAddressString(interface: *const c_void) -> CFStringRef;
}

pub(super) fn links() -> Vec<Link> {
    let hardware = hardware_cards();
    let router = router();
    let wakes = wakes();
    unix::cards()
        .into_iter()
        .filter_map(|(name, card)| {
            let (address, prefix) = card.address?;
            let (medium, hardware) = *hardware.get(&name)?;
            Some(Link {
                index: card.index,
                medium,
                hardware,
                address,
                prefix,
                router: router
                    .as_ref()
                    .filter(|(card, _)| *card == name)
                    .and_then(|(_, hardware)| *hardware),
                wakes,
            })
        })
        .collect()
}

pub(super) const fn fast_startup() -> bool {
    false
}

/// The cards that macOS knows as hardware: a tunnel or a bridge has no entry.
///
/// The socket interface of macOS gives an app a placeholder in place of the hardware address of
/// a card, so the address comes from here.
fn hardware_cards() -> BTreeMap<String, (Medium, Option<Hardware>)> {
    // SAFETY: the call takes no arguments and gives an array that this code owns, or null.
    let all = unsafe { SCNetworkInterfaceCopyAll() };
    if all.is_null() {
        return BTreeMap::new();
    }
    // SAFETY: the array is not null, and the copy rule gives its ownership to this code.
    let all = unsafe { CFArray::<*const c_void>::wrap_under_create_rule(all) };
    let text = |text: CFStringRef| {
        // SAFETY: the string is not null and belongs to its interface, which the array keeps.
        (!text.is_null()).then(|| unsafe { CFString::wrap_under_get_rule(text) }.to_string())
    };
    all.iter()
        .filter_map(|interface| {
            // SAFETY: each item of the array is a network interface that the array keeps alive.
            let (name, kind, address) = unsafe {
                (
                    text(SCNetworkInterfaceGetBSDName(*interface))?,
                    text(SCNetworkInterfaceGetInterfaceType(*interface))?,
                    text(SCNetworkInterfaceGetHardwareAddressString(*interface)),
                )
            };
            let medium = match kind.as_str() {
                "Ethernet" => Medium::Cable,
                "IEEE80211" => Medium::Radio,
                _ => return None,
            };
            let address = address.and_then(|address| hardware(&address, ':'));
            Some((name, (medium, address)))
        })
        .collect()
}

fn output(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The card that has the route to the internet, and the hardware address of its router.
fn router() -> Option<(String, Option<Hardware>)> {
    let route = output("/sbin/route", &["-n", "get", "default"])?;
    let field = |name: &str| {
        route
            .lines()
            .find_map(|line| line.trim().strip_prefix(name))
            .map(|value| value.trim().to_owned())
    };
    let card = field("interface:")?;
    let gateway = field("gateway:")?.parse::<Ipv4Addr>().ok()?;
    let known = output("/usr/sbin/arp", &["-n", &gateway.to_string()])
        .and_then(|line| hardware(line.split_once(" at ")?.1.split_whitespace().next()?, ':'));
    Some((card, known))
}

/// A portable Mac has one value for each power source. The value for the power adapter counts,
/// because a Mac that waits for a packet is at its desk.
fn wakes() -> Wakes {
    let Some(settings) = output("/usr/bin/pmset", &["-g", "custom"]) else {
        return Wakes::Unknown;
    };
    let mut adapter = true;
    let mut found = Wakes::Unknown;
    for line in settings.lines() {
        if !line.starts_with(' ') {
            adapter = line.starts_with("AC Power");
        } else if adapter && let Some(value) = line.trim().strip_prefix("womp") {
            found = if value.trim() == "1" {
                Wakes::On
            } else {
                Wakes::Off
            };
        }
    }
    found
}
