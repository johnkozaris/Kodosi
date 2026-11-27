//! On a Mac the host hands the helper its end of the link in a Mach message.
//!
//! The kernel stamps each Mach message with the audit token of the program that sent it, so the
//! helper learns which program it serves. A Unix socket cannot say who holds it: its peer is the
//! process that last used it, which may since have run another program while a child that it
//! started keeps the socket.

use std::{
    ffi::{CString, c_char, c_int},
    io,
    os::fd::{FromRawFd as _, OwnedFd},
    time::Duration,
};
#[cfg(feature = "host")]
use std::{os::fd::AsRawFd as _, time::Instant};

type Port = u32;

const SUCCESS: i32 = 0;
const SEND: i32 = 0x1;
const RECEIVE: i32 = 0x2;
const SEND_TIMEOUT: i32 = 0x10;
const RECEIVE_TIMEOUT: i32 = 0x100;
/// `MACH_RCV_TRAILER_ELEMENTS(MACH_RCV_TRAILER_AUDIT)` in the default trailer format.
const WITH_AUDIT_TOKEN: i32 = 3 << 24;
const RIGHT_RECEIVE: u32 = 1;
const COPY_SEND: u32 = 19;
const MAKE_SEND_ONCE: u32 = 21;
/// A send right as a message carries it, from `MACH_MSG_TYPE_MOVE_SEND`.
const PORT_SEND: u8 = 17;
const COMPLEX: u32 = 0x8000_0000;
const PORT_DESCRIPTOR: u8 = 0;
#[cfg(feature = "host")]
const MOVE_SEND: u8 = 17;
#[cfg(feature = "host")]
const MOVE_SEND_ONCE: u32 = 18;
/// A send-once right as a message carries it.
#[cfg(feature = "host")]
const PORT_SEND_ONCE: u32 = 18;
#[cfg(feature = "host")]
const RECEIVE_TIMED_OUT: i32 = 0x1000_4003;
#[cfg(feature = "host")]
const RECEIVE_TOO_LARGE: i32 = 0x1000_4004;

unsafe extern "C" {
    static bootstrap_port: Port;
    static mach_task_self_: Port;
    #[cfg(feature = "host")]
    fn bootstrap_check_in(bootstrap: Port, name: *const c_char, service: *mut Port) -> i32;
    fn bootstrap_look_up(bootstrap: Port, name: *const c_char, service: *mut Port) -> i32;
    fn mach_msg(
        message: *mut Header,
        option: i32,
        send_size: u32,
        receive_size: u32,
        receive_name: Port,
        timeout: u32,
        notify: Port,
    ) -> i32;
    fn mach_msg_destroy(message: *mut Header);
    fn mach_port_allocate(task: Port, right: u32, name: *mut Port) -> i32;
    fn mach_port_mod_refs(task: Port, name: Port, right: u32, delta: i32) -> i32;
    fn mach_port_deallocate(task: Port, name: Port) -> i32;
    #[cfg(feature = "host")]
    fn fileport_makeport(fd: c_int, port: *mut Port) -> c_int;
    fn fileport_makefd(port: Port) -> c_int;
}

/// The program that sent a Mach message, as the kernel saw it then; it stays valid only while
/// that program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditToken(pub [u32; 8]);

#[repr(C)]
#[derive(Default)]
struct Header {
    bits: u32,
    size: u32,
    remote: Port,
    local: Port,
    voucher: Port,
    id: i32,
}

#[repr(C)]
#[derive(Default)]
struct PortDescriptor {
    name: Port,
    pad: u32,
    pad2: u16,
    disposition: u8,
    kind: u8,
}

/// The host's answer: the helper's end of the link, as a port that stands for it.
#[repr(C)]
#[derive(Default)]
struct Handed {
    header: Header,
    descriptors: u32,
    link: PortDescriptor,
}

#[repr(C)]
#[derive(Default)]
struct AuditTrailer {
    kind: u32,
    size: u32,
    sequence: u32,
    sender: [u32; 2],
    audit: [u32; 8],
}

#[repr(C)]
#[derive(Default)]
struct Received<T> {
    message: T,
    trailer: AuditTrailer,
}

/// A port that receives, which goes with its last right.
struct Receiver(Port);

impl Receiver {
    fn new() -> io::Result<Self> {
        let mut port = 0;
        // SAFETY: `port` is valid to write.
        let made = unsafe { mach_port_allocate(task(), RIGHT_RECEIVE, &raw mut port) };
        checked(made, "make a port")?;
        Ok(Self(port))
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        // SAFETY: this task holds the receive right.
        unsafe { mach_port_mod_refs(task(), self.0, RIGHT_RECEIVE, -1) };
    }
}

/// A name in this session's bootstrap namespace at which one helper takes the host's end of a
/// link. The name goes away with it.
#[cfg(feature = "host")]
pub(crate) struct Offer(Receiver);

#[cfg(feature = "host")]
impl Offer {
    pub(crate) fn new(name: &str) -> io::Result<Self> {
        let name = CString::new(name).map_err(io::Error::other)?;
        let mut port = 0;
        // SAFETY: `name` is a C string and `port` is valid to write.
        let registered =
            unsafe { bootstrap_check_in(bootstrap_port, name.as_ptr(), &raw mut port) };
        checked(registered, "register the link")?;
        Ok(Self(Receiver(port)))
    }

    /// Waits for the first helper that asks, and hands it `link`.
    pub(crate) fn hand(self, link: OwnedFd, limit: Duration) -> io::Result<()> {
        let deadline = Instant::now() + limit;
        let answer = loop {
            let mut asked = Received::<Header>::default();
            // SAFETY: the buffer holds `receive_size` bytes, and a message that does not fit is
            // dropped.
            let heard = unsafe {
                mach_msg(
                    &raw mut asked.message,
                    RECEIVE | RECEIVE_TIMEOUT,
                    0,
                    size::<Received<Header>>(),
                    (self.0).0,
                    millis(deadline.saturating_duration_since(Instant::now())),
                    0,
                )
            };
            match heard {
                SUCCESS => {}
                RECEIVE_TIMED_OUT => {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "no helper asked"));
                }
                RECEIVE_TOO_LARGE => continue,
                problem => return Err(failed(problem, "hear the helper")),
            }
            let header = &asked.message;
            if header.bits & COMPLEX == 0 && header.bits & 0x1f == PORT_SEND_ONCE {
                break header.remote;
            }
            // SAFETY: a message that this task received, which nothing uses afterwards.
            unsafe { mach_msg_destroy(&raw mut asked.message) };
        };
        let mut port = 0;
        // SAFETY: `link` is an open file and `port` is valid to write.
        let made = unsafe { fileport_makeport(link.as_raw_fd(), &raw mut port) };
        drop(link);
        if made != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut handed = Handed {
            header: Header {
                bits: MOVE_SEND_ONCE | COMPLEX,
                size: size::<Handed>(),
                remote: answer,
                ..Header::default()
            },
            descriptors: 1,
            link: PortDescriptor {
                name: port,
                disposition: MOVE_SEND,
                kind: PORT_DESCRIPTOR,
                ..PortDescriptor::default()
            },
        };
        // SAFETY: `handed` is a whole message of `send_size` bytes.
        let sent = unsafe {
            mach_msg(
                &raw mut handed.header,
                SEND | SEND_TIMEOUT,
                handed.header.size,
                0,
                0,
                0,
                0,
            )
        };
        if sent != SUCCESS {
            // SAFETY: the message did not go, so this task still holds both rights.
            unsafe {
                mach_port_deallocate(task(), port);
                mach_port_deallocate(task(), answer);
            }
        }
        checked(sent, "hand over the link")
    }
}

/// Asks the host that registered `name` for its end of the link, and returns it with the token of
/// the program that handed it over.
pub fn take(name: &str, limit: Duration) -> io::Result<(OwnedFd, AuditToken)> {
    let name = CString::new(name).map_err(io::Error::other)?;
    let mut service = 0;
    // SAFETY: `name` is a C string and `service` is valid to write.
    let found = unsafe { bootstrap_look_up(bootstrap_port, name.as_ptr(), &raw mut service) };
    checked(found, "find the host")?;
    let answers = Receiver::new()?;
    let mut ask = Header {
        bits: COPY_SEND | (MAKE_SEND_ONCE << 8),
        size: size::<Header>(),
        remote: service,
        local: answers.0,
        ..Header::default()
    };
    // SAFETY: `ask` is a whole message of `send_size` bytes.
    let asked = unsafe {
        mach_msg(
            &raw mut ask,
            SEND | SEND_TIMEOUT,
            ask.size,
            0,
            0,
            millis(limit),
            0,
        )
    };
    // SAFETY: this task holds the send right that the lookup gave it.
    unsafe { mach_port_deallocate(task(), service) };
    checked(asked, "ask the host")?;
    let mut handed = Received::<Handed>::default();
    // SAFETY: the buffer holds `receive_size` bytes, and a message that does not fit is dropped.
    let heard = unsafe {
        mach_msg(
            &raw mut handed.message.header,
            RECEIVE | RECEIVE_TIMEOUT | WITH_AUDIT_TOKEN,
            0,
            size::<Received<Handed>>(),
            answers.0,
            millis(limit),
            0,
        )
    };
    checked(heard, "hear the host")?;
    let message = &handed.message;
    let whole = message.header.bits & COMPLEX != 0
        && message.header.size == size::<Handed>()
        && message.descriptors == 1
        && message.link.kind == PORT_DESCRIPTOR
        && message.link.disposition == PORT_SEND
        && handed.trailer.size >= size::<AuditTrailer>();
    if !whole {
        // SAFETY: a message that this task received, which nothing uses afterwards.
        unsafe { mach_msg_destroy(&raw mut handed.message.header) };
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the host handed over no link",
        ));
    }
    let port = message.link.name;
    // SAFETY: the message carried a send right to `port`, which this task then lets go.
    let fd = unsafe {
        let fd = fileport_makefd(port);
        mach_port_deallocate(task(), port);
        fd
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fileport_makefd` opened `fd` for this task alone.
    let link = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok((link, AuditToken(handed.trailer.audit)))
}

fn task() -> Port {
    // SAFETY: the system sets the task's own port before `main`, and never changes it.
    unsafe { mach_task_self_ }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a message is a few dozen bytes"
)]
const fn size<T>() -> u32 {
    size_of::<T>() as u32
}

fn millis(limit: Duration) -> u32 {
    u32::try_from(limit.as_millis()).unwrap_or(u32::MAX)
}

fn checked(result: i32, doing: &str) -> io::Result<()> {
    if result == SUCCESS {
        Ok(())
    } else {
        Err(failed(result, doing))
    }
}

fn failed(result: i32, doing: &str) -> io::Error {
    io::Error::other(format!("could not {doing}: Mach error {result:#x}"))
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        os::unix::net::UnixStream,
    };

    use super::*;

    #[test]
    fn a_helper_takes_the_hosts_end_of_the_link_with_the_hosts_audit_token() {
        let name = format!("com.kodosi.screen.test.{}", std::process::id());
        let offer = Offer::new(&name).unwrap();
        let (mut host, helper) = UnixStream::pair().unwrap();
        let handing = std::thread::spawn(move || offer.hand(helper.into(), Duration::from_secs(5)));
        let (link, token) = take(&name, Duration::from_secs(5)).unwrap();
        handing.join().unwrap().unwrap();
        assert_eq!(token.0[5], std::process::id());
        host.write_all(b"hello").unwrap();
        let mut heard = [0; 5];
        UnixStream::from(link).read_exact(&mut heard).unwrap();
        assert_eq!(&heard, b"hello");
    }
}
