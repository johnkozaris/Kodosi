use std::{
    io,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    time::Duration,
};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
use android as imp;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
use ios as imp;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(all(unix, not(target_os = "ios")))]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod service;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use service::carry_as_screen_sharing;

pub type Hardware = [u8; 6];

const WAKE_PORT: u16 = 9;
const ROUNDS: usize = 3;
const BETWEEN_ROUNDS: Duration = Duration::from_millis(100);
const LONGEST_PREFIX: u8 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Medium {
    Cable,
    Radio,
}

/// Whether the operating system lets a packet from the network wake the computer through a card.
/// The firmware has its own setting, which no system shows to a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wakes {
    On,
    Off,
    Unknown,
}

/// A network card of this device that is connected now and has an IPv4 address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The number that the system gives the card, or 0 when the system does not tell it.
    pub index: u32,
    pub medium: Medium,
    /// A phone does not show the hardware address of its card to an app.
    pub hardware: Option<Hardware>,
    pub address: Ipv4Addr,
    pub prefix: u8,
    /// The hardware address of the router, when this card has the route to the internet and the
    /// system shows it.
    pub router: Option<Hardware>,
    pub wakes: Wakes,
}

impl Link {
    pub fn broadcast(&self) -> Ipv4Addr {
        let mask = u32::MAX
            .checked_shl(32 - u32::from(self.prefix))
            .unwrap_or(0);
        Ipv4Addr::from_bits(self.address.to_bits() | !mask)
    }
}

pub fn links() -> Vec<Link> {
    let mut links = imp::links();
    links.retain(|link| (1..=LONGEST_PREFIX).contains(&link.prefix));
    links
}

/// Windows with Fast Startup keeps a shut-down computer in a state from which no packet wakes it.
pub fn fast_startup() -> bool {
    imp::fast_startup()
}

/// Sends the magic packet for each hardware address out of one card.
///
/// It goes to the network of the card and to the limited broadcast address, with and without an
/// empty password: some cards wake only with the password field.
pub fn wake(from: &Link, sleepers: &[Hardware]) -> io::Result<()> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_broadcast(true)?;
    socket.bind(&SocketAddrV4::new(from.address, 0).into())?;
    // Without this, macOS sends the limited broadcast out of the card with the default route.
    #[cfg(unix)]
    if let Some(index) = std::num::NonZeroU32::new(from.index) {
        drop(socket.bind_device_by_index_v4(Some(index)));
    }
    let socket = UdpSocket::from(socket);
    let network = SocketAddrV4::new(from.broadcast(), WAKE_PORT);
    let everywhere = SocketAddrV4::new(Ipv4Addr::BROADCAST, WAKE_PORT);
    for round in 0..ROUNDS {
        if round > 0 {
            std::thread::sleep(BETWEEN_ROUNDS);
        }
        for sleeper in sleepers {
            let packet = magic_packet(*sleeper);
            for form in [&packet[..], &packet[..PLAIN]] {
                socket.send_to(form, network)?;
                // A system can refuse this address on a card that is not its main one. The packet
                // to the network of the card is enough then.
                drop(socket.send_to(form, everywhere));
            }
        }
    }
    Ok(())
}

/// Whether the system refused a send because the person did not let this program onto the local
/// network: macOS answers that the device is unreachable.
pub fn refused_on_local_network(error: &io::Error) -> bool {
    cfg!(target_os = "macos") && error.kind() == io::ErrorKind::HostUnreachable
}

const PLAIN: usize = 6 + 16 * 6;
const PASSWORD: usize = 6;

fn magic_packet(sleeper: Hardware) -> [u8; PLAIN + PASSWORD] {
    let mut packet = [0; PLAIN + PASSWORD];
    packet[..6].fill(0xff);
    for copy in packet[6..PLAIN].as_chunks_mut::<6>().0 {
        *copy = sleeper;
    }
    packet
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn hardware(text: &str, separator: char) -> Option<Hardware> {
    let mut hardware = [0; 6];
    let mut parts = text.trim().split(separator);
    for byte in &mut hardware {
        *byte = u8::from_str_radix(parts.next()?, 16).ok()?;
    }
    (parts.next().is_none() && hardware != [0; 6]).then_some(hardware)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hardware_address_reads_with_short_parts_and_refuses_an_empty_one() {
        assert_eq!(
            hardware("0:e0:4c:e4:88:7c", ':'),
            Some([0x00, 0xe0, 0x4c, 0xe4, 0x88, 0x7c])
        );
        assert_eq!(
            hardware("3C-F7-5D-56-34-60", '-'),
            Some([0x3c, 0xf7, 0x5d, 0x56, 0x34, 0x60])
        );
        assert_eq!(hardware("00:00:00:00:00:00", ':'), None);
        assert_eq!(hardware("(incomplete)", ':'), None);
        assert_eq!(hardware("3c:f7:5d:56:34", ':'), None);
        assert_eq!(hardware("3c:f7:5d:56:34:60:01", ':'), None);
    }

    #[test]
    fn the_magic_packet_has_the_address_sixteen_times_and_an_empty_password() {
        let sleeper = [0x3c, 0xf7, 0x5d, 0x56, 0x34, 0x60];
        let packet = magic_packet(sleeper);
        assert_eq!(packet.len(), 108);
        assert_eq!(packet[..6], [0xff; 6]);
        assert!(packet[6..102].chunks(6).all(|copy| copy == sleeper));
        assert_eq!(packet[6..102].len() / 6, 16);
        assert_eq!(packet[102..], [0; 6]);
    }

    #[test]
    fn a_card_sends_to_the_last_address_of_its_network() {
        let link = |address, prefix| Link {
            index: 0,
            medium: Medium::Cable,
            hardware: None,
            address,
            prefix,
            router: None,
            wakes: Wakes::Unknown,
        };
        assert_eq!(
            link(Ipv4Addr::new(192, 168, 1, 20), 24).broadcast(),
            Ipv4Addr::new(192, 168, 1, 255)
        );
        assert_eq!(
            link(Ipv4Addr::new(10, 4, 130, 7), 22).broadcast(),
            Ipv4Addr::new(10, 4, 131, 255)
        );
    }
}
