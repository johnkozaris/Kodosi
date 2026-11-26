#![allow(unsafe_code)]

use std::{ffi::CStr, net::Ipv4Addr, ptr};

use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
    NetworkManagement::{
        IpHelper::{
            GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
            GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, GetIfEntry2, IF_TYPE_ETHERNET_CSMACD,
            IF_TYPE_IEEE80211, IP_ADAPTER_ADDRESSES_LH, MIB_IF_ROW2, SendARP,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{AF_INET, SOCKADDR, SOCKADDR_IN},
    System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey,
        RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
    },
};

use super::{Hardware, Link, Medium, Wakes};
use crate::win;

const NETWORK_CARDS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";
const POWER: &str = r"SYSTEM\CurrentControlSet\Control\Power";
const SESSION_POWER: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Power";
const CONNECTOR_PRESENT: u8 = 0b100;
/// The card's power management is off, or the card must not wake the computer.
const NO_WAKE: u32 = 0x18;
const FIRST_BUFFER: u32 = 16 * 1024;
const MOST_CARDS: u32 = 256;

pub(super) fn links() -> Vec<Link> {
    let flags = GAA_FLAG_INCLUDE_GATEWAYS
        | GAA_FLAG_SKIP_ANYCAST
        | GAA_FLAG_SKIP_MULTICAST
        | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = FIRST_BUFFER;
    let mut buffer = Vec::<u64>::new();
    for _ in 0..3 {
        buffer.resize((size as usize).div_ceil(8), 0);
        // SAFETY: the buffer has at least `size` bytes and the alignment of the list's entries.
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET),
                flags,
                ptr::null(),
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        match status {
            ERROR_SUCCESS => return read(buffer.as_ptr().cast()),
            ERROR_BUFFER_OVERFLOW => {}
            _ => break,
        }
    }
    Vec::new()
}

fn read(mut adapter: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<Link> {
    let mut links = Vec::new();
    // SAFETY: Windows filled the buffer with a list that ends with a null pointer, and each
    // pointer in it stays inside the buffer, which the caller keeps for this call.
    while let Some(card) = unsafe { adapter.as_ref() } {
        adapter = card.Next;
        let medium = match card.IfType {
            IF_TYPE_ETHERNET_CSMACD => Medium::Cable,
            IF_TYPE_IEEE80211 => Medium::Radio,
            _ => continue,
        };
        // SAFETY: both members of the union start with the length and the index.
        let index = unsafe { card.Anonymous1.Anonymous.IfIndex };
        if card.OperStatus != IfOperStatusUp
            || card.PhysicalAddressLength != 6
            || !has_connector(index)
        {
            continue;
        }
        let Some((address, prefix)) = unicast(card) else {
            continue;
        };
        let mut hardware = [0; 6];
        hardware.copy_from_slice(&card.PhysicalAddress[..6]);
        // SAFETY: the name is a string that ends with a zero byte, inside the buffer.
        let name = unsafe { CStr::from_ptr(card.AdapterName.cast()) }.to_string_lossy();
        links.push(Link {
            index,
            medium,
            hardware: Some(hardware),
            address,
            prefix,
            router: gateway(card).and_then(neighbor),
            wakes: wakes(&name),
        });
    }
    links
}

/// Software makes a card that has no connector: a virtual switch or a tunnel.
fn has_connector(index: u32) -> bool {
    let mut row = MIB_IF_ROW2 {
        InterfaceIndex: index,
        ..Default::default()
    };
    // SAFETY: the row is a valid place for the result and names its card by the index.
    let read = unsafe { GetIfEntry2(&raw mut row) } == ERROR_SUCCESS;
    read && row.InterfaceAndOperStatusFlags._bitfield & CONNECTOR_PRESENT != 0
}

fn internet(address: *const SOCKADDR) -> Option<Ipv4Addr> {
    // SAFETY: the address is null or a socket address that Windows wrote into the buffer.
    let family = unsafe { address.as_ref() }?.sa_family;
    (family == AF_INET).then(|| {
        // SAFETY: a socket address of this family has the IPv4 layout.
        let internet = unsafe { address.cast::<SOCKADDR_IN>().read_unaligned() };
        // SAFETY: each form of the address is the same four bytes.
        Ipv4Addr::from(unsafe { internet.sin_addr.S_un.S_addr }.to_ne_bytes())
    })
}

fn unicast(card: &IP_ADAPTER_ADDRESSES_LH) -> Option<(Ipv4Addr, u8)> {
    let mut next = card.FirstUnicastAddress;
    // SAFETY: the list ends with a null pointer, and its entries are inside the buffer.
    while let Some(entry) = unsafe { next.as_ref() } {
        next = entry.Next;
        if let Some(address) = internet(entry.Address.lpSockaddr)
            && !address.is_link_local()
        {
            return Some((address, entry.OnLinkPrefixLength));
        }
    }
    None
}

fn gateway(card: &IP_ADAPTER_ADDRESSES_LH) -> Option<Ipv4Addr> {
    let mut next = card.FirstGatewayAddress;
    // SAFETY: the list ends with a null pointer, and its entries are inside the buffer.
    while let Some(entry) = unsafe { next.as_ref() } {
        next = entry.Next;
        if let Some(address) = internet(entry.Address.lpSockaddr) {
            return Some(address);
        }
    }
    None
}

fn neighbor(address: Ipv4Addr) -> Option<Hardware> {
    let mut hardware = [0_u8; 8];
    let mut length = 8;
    // SAFETY: the buffer has `length` bytes, and the call writes at most that many.
    let status = unsafe {
        SendARP(
            u32::from_ne_bytes(address.octets()),
            0,
            hardware.as_mut_ptr().cast(),
            &raw mut length,
        )
    };
    let found = [
        hardware[0],
        hardware[1],
        hardware[2],
        hardware[3],
        hardware[4],
        hardware[5],
    ];
    (status == ERROR_SUCCESS && length == 6 && found != [0; 6]).then_some(found)
}

struct Key(HKEY);

impl Key {
    fn open(path: &str) -> Option<Self> {
        let mut key: HKEY = ptr::null_mut();
        // SAFETY: the path is a string that ends with zero, and `key` is a valid place for the
        // result.
        (unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                win::wide(path).as_ptr(),
                0,
                KEY_READ,
                &raw mut key,
            )
        } == ERROR_SUCCESS)
            .then_some(Self(key))
    }

    fn folder(&self, index: u32) -> Option<String> {
        let mut name = [0_u16; 256];
        let mut length = 256;
        // SAFETY: the name buffer has `length` characters, and the other results are not asked
        // for.
        (unsafe {
            RegEnumKeyExW(
                self.0,
                index,
                name.as_mut_ptr(),
                &raw mut length,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } == ERROR_SUCCESS)
            .then(|| String::from_utf16_lossy(&name[..length as usize]))
    }

    fn text(&self, folder: &str, value: &str) -> Option<String> {
        let mut text = [0_u16; 256];
        let mut bytes = 512;
        // SAFETY: both names end with zero, and the buffer has `bytes` bytes.
        (unsafe {
            RegGetValueW(
                self.0,
                win::wide(folder).as_ptr(),
                win::wide(value).as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                text.as_mut_ptr().cast(),
                &raw mut bytes,
            )
        } == ERROR_SUCCESS)
            .then(|| {
                let length = (bytes as usize / 2).min(text.len());
                String::from_utf16_lossy(&text[..length])
                    .trim_end_matches('\0')
                    .to_owned()
            })
    }

    fn number(&self, folder: &str, value: &str) -> Option<u32> {
        let mut number = 0_u32;
        let mut bytes = 4;
        // SAFETY: both names end with zero, and the buffer is one number of four bytes.
        (unsafe {
            RegGetValueW(
                self.0,
                win::wide(folder).as_ptr(),
                win::wide(value).as_ptr(),
                RRF_RT_REG_DWORD,
                ptr::null_mut(),
                (&raw mut number).cast(),
                &raw mut bytes,
            )
        } == ERROR_SUCCESS)
            .then_some(number)
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key is the open one that `open` made.
        unsafe { RegCloseKey(self.0) };
    }
}

/// The settings of the card's driver, which the Power Management page of the card shows.
fn wakes(card: &str) -> Wakes {
    let Some(cards) = Key::open(NETWORK_CARDS) else {
        return Wakes::Unknown;
    };
    let Some(folder) = (0..MOST_CARDS)
        .map_while(|index| cards.folder(index))
        .find(|folder| {
            cards
                .text(folder, "NetCfgInstanceId")
                .is_some_and(|id| id.eq_ignore_ascii_case(card))
        })
    else {
        return Wakes::Unknown;
    };
    let packet = cards.text(&folder, "*WakeOnMagicPacket");
    let refused = cards
        .number(&folder, "PnPCapabilities")
        .is_some_and(|capabilities| capabilities & NO_WAKE != 0);
    match packet.as_deref() {
        Some("0") => Wakes::Off,
        _ if refused => Wakes::Off,
        Some("1") => Wakes::On,
        _ => Wakes::Unknown,
    }
}

/// Fast Startup is a hibernation of the system at shutdown, so its setting counts only on a
/// computer where hibernation is on.
pub(super) fn fast_startup() -> bool {
    let on = |path, value| Key::open(path).is_some_and(|key| key.number("", value) == Some(1));
    on(SESSION_POWER, "HiberbootEnabled") && on(POWER, "HibernateEnabled")
}
