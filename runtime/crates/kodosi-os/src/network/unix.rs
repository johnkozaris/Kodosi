use std::{collections::BTreeMap, net::Ipv4Addr};

use nix::{ifaddrs::getifaddrs, net::if_::InterfaceFlags};

#[cfg(target_os = "linux")]
use super::Hardware;

#[derive(Default)]
pub(super) struct Card {
    pub(super) index: u32,
    /// Only Linux gives an app the hardware address of a card here.
    #[cfg(target_os = "linux")]
    pub(super) hardware: Option<Hardware>,
    pub(super) address: Option<(Ipv4Addr, u8)>,
}

/// The cards that are up and can broadcast, by name, with the first IPv4 address of each that is
/// not link-local.
pub(super) fn cards() -> BTreeMap<String, Card> {
    let mut cards = BTreeMap::<String, Card>::new();
    let wanted =
        InterfaceFlags::IFF_UP | InterfaceFlags::IFF_RUNNING | InterfaceFlags::IFF_BROADCAST;
    for entry in getifaddrs().into_iter().flatten() {
        if entry.flags.contains(InterfaceFlags::IFF_LOOPBACK) || !entry.flags.contains(wanted) {
            continue;
        }
        let Some(address) = entry.address else {
            continue;
        };
        let index = nix::net::if_::if_nametoindex(entry.interface_name.as_str()).unwrap_or(0);
        let card = cards.entry(entry.interface_name).or_default();
        card.index = index;
        #[cfg(target_os = "linux")]
        if let Some(link) = address.as_link_addr() {
            card.hardware = link.addr().filter(|hardware| *hardware != [0; 6]);
        }
        if let Some(internet) = address.as_sockaddr_in()
            && card.address.is_none()
            && !internet.ip().is_link_local()
        {
            let mask = entry
                .netmask
                .as_ref()
                .and_then(|mask| mask.as_sockaddr_in())
                .map_or(Ipv4Addr::BROADCAST, nix::sys::socket::SockaddrIn::ip);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a mask has at most 32 ones"
            )]
            let prefix = mask.to_bits().leading_ones() as u8;
            card.address = Some((internet.ip(), prefix));
        }
    }
    cards
}
