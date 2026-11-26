#![allow(unsafe_code)]

use std::{io, net::UdpSocket, os::fd::AsRawFd as _};

use nix::libc::{SOL_SOCKET, c_int, c_void, setsockopt, socklen_t};

// From <sys/socket.h>; the libc crate does not define them.
const SO_NET_SERVICE_TYPE: c_int = 0x1116;
const NET_SERVICE_TYPE_RV: c_int = 5;

/// Has the system carry the socket's traffic as screen sharing.
///
/// The traffic then leaves ahead of the computer's bulk traffic and with video priority on
/// Wi-Fi. The class stays on this computer, unlike a DSCP mark, which some routers drop.
pub fn carry_as_screen_sharing(socket: &UdpSocket) -> io::Result<()> {
    let class = NET_SERVICE_TYPE_RV;
    let result = unsafe {
        setsockopt(
            socket.as_raw_fd(),
            SOL_SOCKET,
            SO_NET_SERVICE_TYPE,
            (&raw const class).cast::<c_void>(),
            socklen_t::try_from(size_of::<c_int>()).map_err(io::Error::other)?,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
