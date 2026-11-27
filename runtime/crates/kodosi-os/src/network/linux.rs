use std::{collections::HashMap, fs, net::Ipv4Addr, path::Path};

use zbus::{Connection, OwnedObjectPath, OwnedVariant};

use super::{Hardware, Link, Medium, Wakes, hardware, unix};
use crate::bus;

const CARDS: &str = "/sys/class/net";
const ETHERNET: &str = "1";
const GATEWAY: u32 = 0x2;
const MANAGER: &str = "org.freedesktop.NetworkManager";
const MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// The flags of `802-3-ethernet.wake-on-lan`.
const WAKE_BROADCAST: u32 = 0x10;
const WAKE_MAGIC: u32 = 0x40;
const WAKE_DEFAULT: u32 = 0x1;
const WAKE_IGNORE: u32 = 0x8000;

pub(super) fn links() -> Vec<Link> {
    let router = router();
    let cards = unix::cards();
    let managed = network_manager(cards.keys().cloned().collect());
    cards
        .into_iter()
        .filter_map(|(name, card)| {
            let folder = Path::new(CARDS).join(&name);
            let read = |file: &str| fs::read_to_string(folder.join(file)).ok();
            // Software makes a card that has no device: a bridge, a tunnel, a container's pair.
            if !folder.join("device").exists() || read("type")?.trim() != ETHERNET {
                return None;
            }
            let (address, prefix) = card.address?;
            let radio = folder.join("wireless").exists() || folder.join("phy80211").exists();
            Some(Link {
                index: card.index,
                medium: if radio { Medium::Radio } else { Medium::Cable },
                hardware: Some(card.hardware?),
                address,
                prefix,
                router: router
                    .as_ref()
                    .filter(|(card, _)| *card == name)
                    .and_then(|(_, hardware)| *hardware),
                wakes: match read("device/power/wakeup").as_deref().map(str::trim) {
                    Some("disabled") => Wakes::Off,
                    device => {
                        let driver = if device == Some("enabled") {
                            Wakes::On
                        } else {
                            Wakes::Unknown
                        };
                        managed
                            .get(&name)
                            .copied()
                            .and_then(decided)
                            .unwrap_or(driver)
                    }
                },
            })
        })
        .collect()
}

pub(super) const fn fast_startup() -> bool {
    false
}

/// The Wake-on-LAN flags of the connection that `NetworkManager` runs on each card. It sets them on
/// the card each time it connects it, and any user may read them.
fn network_manager(cards: Vec<String>) -> HashMap<String, u32> {
    bus::ask_system(async move |bus| {
        let mut managed = HashMap::new();
        for card in cards {
            if let Some(flags) = wake_flags(bus, &card).await {
                managed.insert(card, flags);
            }
        }
        Some(managed)
    })
    .unwrap_or_default()
}

async fn wake_flags(bus: &Connection, card: &str) -> Option<u32> {
    let device: OwnedObjectPath = bus
        .call_method(
            Some(MANAGER),
            MANAGER_PATH,
            Some(MANAGER),
            "GetDeviceByIpIface",
            &card,
        )
        .await
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    let active = path_property(
        bus,
        &device,
        "org.freedesktop.NetworkManager.Device",
        "ActiveConnection",
    )
    .await?;
    let settings = path_property(
        bus,
        &active,
        "org.freedesktop.NetworkManager.Connection.Active",
        "Connection",
    )
    .await?;
    let mut all: HashMap<String, HashMap<String, OwnedVariant>> = bus
        .call_method(
            Some(MANAGER),
            &settings,
            Some("org.freedesktop.NetworkManager.Settings.Connection"),
            "GetSettings",
            &(),
        )
        .await
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    Some(
        all.remove("802-3-ethernet")
            .and_then(|wired| {
                wired
                    .get("wake-on-lan")
                    .and_then(|flags| u32::try_from(flags).ok())
            })
            .unwrap_or(WAKE_DEFAULT),
    )
}

async fn path_property(
    bus: &Connection,
    path: &OwnedObjectPath,
    interface: &str,
    name: &str,
) -> Option<OwnedObjectPath> {
    let value: OwnedVariant = bus
        .call_method(
            Some(MANAGER),
            path,
            Some(PROPERTIES),
            "Get",
            &(interface, name),
        )
        .await
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    OwnedObjectPath::try_from(value).ok()
}

/// What the flags of `NetworkManager` say about a magic packet. It leaves the card as its driver set
/// it when the flags ask for the default or to be left alone.
const fn decided(flags: u32) -> Option<Wakes> {
    if flags == WAKE_DEFAULT || flags & WAKE_IGNORE != 0 {
        None
    } else if flags & (WAKE_MAGIC | WAKE_BROADCAST) != 0 {
        Some(Wakes::On)
    } else {
        Some(Wakes::Off)
    }
}

/// The card that has the route to the internet, and the hardware address of its router.
fn router() -> Option<(String, Option<Hardware>)> {
    let (card, gateway) = default_route(&fs::read_to_string("/proc/net/route").ok()?)?;
    let known = fs::read_to_string("/proc/net/arp")
        .ok()
        .and_then(|neighbors| neighbor(&neighbors, &card, gateway));
    Some((card, known))
}

/// The kernel writes each address of this table as the hexadecimal digits of its bytes in memory.
fn default_route(routes: &str) -> Option<(String, Ipv4Addr)> {
    routes
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let number = |index: usize| u32::from_str_radix(fields.get(index)?, 16).ok();
            let default = number(1)? == 0 && number(7)? == 0 && number(3)? & GATEWAY != 0;
            let metric = fields.get(6)?.parse::<u32>().ok()?;
            default.then_some((
                metric,
                (*fields.first()?).to_owned(),
                Ipv4Addr::from(number(2)?.to_ne_bytes()),
            ))
        })
        .min_by_key(|(metric, ..)| *metric)
        .map(|(_, card, gateway)| (card, gateway))
}

fn neighbor(neighbors: &str, card: &str, address: Ipv4Addr) -> Option<Hardware> {
    neighbors.lines().skip(1).find_map(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        (fields.first()?.parse() == Ok(address) && fields.get(5) == Some(&card))
            .then(|| hardware(fields.get(3)?, ':'))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_route_to_the_internet_names_its_card_and_its_router() {
        let routes = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
            wlp2s0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
            enp1s0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
            enp1s0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
        assert_eq!(
            default_route(routes),
            Some(("enp1s0".to_owned(), Ipv4Addr::new(192, 168, 1, 1)))
        );
        let neighbors = "IP address       HW type     Flags       HW address            Mask     Device\n\
            192.168.1.1      0x1         0x2         3c:f7:5d:56:34:60     *        wlp2s0\n\
            192.168.1.9      0x1         0x0         00:00:00:00:00:00     *        enp1s0\n\
            192.168.1.1      0x1         0x2         3c:f7:5d:56:34:61     *        enp1s0\n";
        assert_eq!(
            neighbor(neighbors, "enp1s0", Ipv4Addr::new(192, 168, 1, 1)),
            Some([0x3c, 0xf7, 0x5d, 0x56, 0x34, 0x61])
        );
        assert_eq!(
            neighbor(neighbors, "enp1s0", Ipv4Addr::new(192, 168, 1, 9)),
            None
        );
    }

    #[test]
    fn network_manager_decides_the_wake_only_when_it_sets_the_card() {
        assert!(matches!(decided(WAKE_MAGIC), Some(Wakes::On)));
        assert!(matches!(decided(0), Some(Wakes::Off)));
        assert!(
            matches!(decided(0x2), Some(Wakes::Off)),
            "a wake on a link change"
        );
        assert!(decided(WAKE_DEFAULT).is_none());
        assert!(decided(WAKE_IGNORE).is_none());
    }
}
