//! The Kodosi server's UDP relay, which forwards QUIC between two devices that reach no direct
//! path. noq sees the other device at a stand-in address; the socket puts the relay's header on
//! what goes there and takes it off what comes back.

use std::{collections::HashMap, net::SocketAddr};

use aws_lc_rs::hmac;
use serde_json::Value;
use uuid::Uuid;

use super::*;

pub(super) const DATA: u8 = 0x10;
const BIND: u8 = 0x11;
pub(super) const BOUND: u8 = 0x12;
const ALLOCATION: usize = 7;
const HEADER: usize = 1 + ALLOCATION;
const SECRET: usize = 32;
const MAC: usize = 16;
const BIND_BYTES: usize = HEADER + 1 + 8 + MAC;
/// Binds go out this often until the relay answers one, a few times, and then as often as it
/// takes to keep the router's mapping and the relay's slot on this device's address.
const REBIND: Duration = Duration::from_secs(1);
const QUICK_BINDS: u32 = 5;
const KEEP_BOUND: Duration = Duration::from_secs(15);
/// Stand-ins come from a unique local IPv6 prefix, or from 240.0.0.0/8, which IPv4 reserves.
const STAND_IN_V6: [u16; 4] = [0xfd6b, 0x6f64, 0x6f73, 0x6900];
const STAND_IN_V4: u8 = 240;
const STAND_IN_PORT: u16 = 3478;

/// The allocation that the server gave this device for another one, this device's slot in it,
/// the slot's secret, and the server's time when it gave them, which times the binds.
#[derive(Clone, Debug)]
pub(crate) struct Grant {
    allocation: [u8; ALLOCATION],
    slot: u8,
    secret: [u8; SECRET],
    server_time: i64,
    given: tokio::time::Instant,
}

impl Grant {
    pub(crate) fn read(value: &Value) -> Result<(Uuid, Self)> {
        let pipe = wire::id(value, "pipe")?;
        let allocation = wire::decode_b64(value, "allocation", ALLOCATION)?
            .try_into()
            .map_err(|_| invalid("Invalid relay allocation."))?;
        let secret = wire::decode_b64(value, "secret", SECRET)?
            .try_into()
            .map_err(|_| invalid("Invalid relay secret."))?;
        let slot = value
            .get("slot")
            .and_then(Value::as_u64)
            .filter(|slot| *slot <= 1)
            .and_then(|slot| u8::try_from(slot).ok())
            .ok_or_else(|| invalid("Invalid relay slot."))?;
        let server_time = value
            .get("now")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid("Invalid relay time."))?;
        Ok((
            pipe,
            Self {
                allocation,
                slot,
                secret,
                server_time,
                given: tokio::time::Instant::now(),
            },
        ))
    }

    fn bind(&self) -> [u8; BIND_BYTES] {
        let since = i64::try_from(self.given.elapsed().as_millis()).unwrap_or(i64::MAX);
        self.bind_at(self.server_time.saturating_add(since))
    }

    /// A bind signed with the slot's secret, for the server's `time`.
    fn bind_at(&self, time: i64) -> [u8; BIND_BYTES] {
        let mut packet = [0; BIND_BYTES];
        packet[0] = BIND;
        packet[1..HEADER].copy_from_slice(&self.allocation);
        packet[HEADER] = self.slot;
        packet[HEADER + 1..HEADER + 9].copy_from_slice(&time.to_be_bytes());
        let tag = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, &self.secret),
            &packet[..BIND_BYTES - MAC],
        );
        packet[BIND_BYTES - MAC..].copy_from_slice(&tag.as_ref()[..MAC]);
        packet
    }
}

#[derive(Debug)]
struct Route {
    server: SocketAddr,
    allocation: [u8; ALLOCATION],
    slot: u8,
    bound: bool,
}

/// The allocations that one socket relays, by the stand-in of the device at the other end.
#[derive(Debug, Default)]
pub(super) struct Routes {
    by_stand_in: HashMap<SocketAddr, Route>,
    by_allocation: HashMap<[u8; ALLOCATION], SocketAddr>,
    made: u32,
}

/// Relayed segments of one received batch that belong to another allocation than those before
/// them, for the next receive: one batch carries only one source address.
#[derive(Debug)]
pub(super) struct Leftover {
    data: Vec<u8>,
    stride: usize,
    from: SocketAddr,
}

enum Segment {
    Data(SocketAddr),
    Other,
}

impl Routes {
    /// The stand-in for `grant`'s allocation, and whether it is new.
    fn add(&mut self, grant: &Grant, server: SocketAddr, dual: bool) -> (SocketAddr, bool) {
        if let Some(stand_in) = self.by_allocation.get(&grant.allocation) {
            return (*stand_in, false);
        }
        self.made = self.made.wrapping_add(1);
        let [high, middle, low, last] = self.made.to_be_bytes();
        let stand_in = if dual {
            let [a, b, c, d] = STAND_IN_V6;
            SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(
                    a,
                    b,
                    c,
                    d,
                    0,
                    0,
                    u16::from_be_bytes([high, middle]),
                    u16::from_be_bytes([low, last]),
                )),
                STAND_IN_PORT,
            )
        } else {
            SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(STAND_IN_V4, middle, low, last)),
                STAND_IN_PORT,
            )
        };
        self.by_stand_in.insert(
            stand_in,
            Route {
                server: plain(server),
                allocation: grant.allocation,
                slot: grant.slot,
                bound: false,
            },
        );
        self.by_allocation.insert(grant.allocation, stand_in);
        (stand_in, true)
    }

    fn remove(&mut self, allocation: [u8; ALLOCATION]) {
        if let Some(stand_in) = self.by_allocation.remove(&allocation) {
            self.by_stand_in.remove(&stand_in);
        }
    }

    pub(super) fn bound(&self, stand_in: SocketAddr) -> Option<bool> {
        self.by_stand_in.get(&stand_in).map(|route| route.bound)
    }

    /// Where a packet for `destination` goes and the header it takes, when `destination` is a
    /// stand-in.
    pub(super) fn wrap(&self, destination: SocketAddr) -> Option<(SocketAddr, [u8; HEADER])> {
        let route = self.by_stand_in.get(&plain(destination))?;
        let mut header = [DATA; HEADER];
        header[1..].copy_from_slice(&route.allocation);
        Some((route.server, header))
    }

    /// Takes the relay's header off the received segments in `buffer`, as far as they belong to
    /// one allocation, and gives their stand-in, length and stride; the segments after them are
    /// left over.
    pub(super) fn unwrap(
        &mut self,
        buffer: &mut [u8],
        from: SocketAddr,
        stride: usize,
    ) -> (Option<(SocketAddr, usize, usize)>, Option<Leftover>) {
        let from = plain(from);
        if stride <= HEADER || !self.by_stand_in.values().any(|route| route.server == from) {
            return (None, None);
        }
        let (mut run, mut written, mut leftover) = (None, 0, None);
        for start in (0..buffer.len()).step_by(stride) {
            let end = (start + stride).min(buffer.len());
            match self.segment(&buffer[start..end]) {
                Segment::Data(stand_in) if run.is_none_or(|run| run == stand_in) => {
                    run = Some(stand_in);
                    buffer.copy_within(start + HEADER..end, written);
                    written += end - start - HEADER;
                }
                Segment::Data(_) => {
                    leftover = Some(Leftover {
                        data: buffer[start..].to_vec(),
                        stride,
                        from,
                    });
                    break;
                }
                Segment::Other => {}
            }
        }
        (
            run.map(|stand_in| (stand_in, written, stride - HEADER)),
            leftover,
        )
    }

    fn segment(&mut self, segment: &[u8]) -> Segment {
        let Some(stand_in) = segment
            .get(1..HEADER)
            .and_then(|allocation| self.by_allocation.get(allocation))
            .copied()
        else {
            return Segment::Other;
        };
        match segment[0] {
            DATA if segment.len() > HEADER => Segment::Data(stand_in),
            BOUND if segment.len() == HEADER + 1 => {
                if let Some(route) = self.by_stand_in.get_mut(&stand_in)
                    && route.slot == segment[HEADER]
                {
                    route.bound = true;
                }
                Segment::Other
            }
            _ => Segment::Other,
        }
    }
}

impl Leftover {
    /// Moves the leftover segments into `buffer` for another unwrap.
    pub(super) fn into_buffer(self, buffer: &mut [u8]) -> Option<(SocketAddr, usize, usize)> {
        let target = buffer.get_mut(..self.data.len())?;
        target.copy_from_slice(&self.data);
        Some((self.from, self.data.len(), self.stride))
    }
}

/// Wraps each segment of `contents` in the relay's header.
pub(super) fn wrapped(header: [u8; HEADER], contents: &[u8], segment: Option<usize>) -> Vec<u8> {
    let size = segment.unwrap_or(contents.len()).max(1);
    let mut packet = Vec::with_capacity(contents.len() + contents.len().div_ceil(size) * HEADER);
    for chunk in contents.chunks(size) {
        packet.extend_from_slice(&header);
        packet.extend_from_slice(chunk);
    }
    packet
}

pub(super) const fn wrapped_segment(segment: Option<usize>) -> Option<usize> {
    match segment {
        Some(size) => Some(size + HEADER),
        None => None,
    }
}

/// An address as the socket compares it: IPv4 addresses that a dual-stack socket maps into IPv6
/// count as IPv4.
fn plain(address: SocketAddr) -> SocketAddr {
    match address {
        SocketAddr::V6(v6) => v6
            .ip()
            .to_ipv4_mapped()
            .map_or(address, |v4| SocketAddr::new(IpAddr::V4(v4), v6.port())),
        SocketAddr::V4(_) => address,
    }
}

impl Peers {
    /// Takes the allocation that the server gave for the devices of `pipe`.
    pub(crate) fn relayed(&self, pipe: Uuid, grant: Grant) {
        if let Ok(mut grants) = self.grants.lock() {
            grants.insert(pipe, grant);
        }
    }

    /// Forgets the allocation of `pipe`, and stops relaying it when no other pipe uses it. A
    /// socket that is busy at the moment keeps the route until it closes.
    pub(crate) fn forget_relayed(&self, pipe: Uuid) {
        let Some(gone) = self
            .grants
            .lock()
            .ok()
            .and_then(|mut grants| grants.remove(&pipe))
        else {
            return;
        };
        let used = self.grants.lock().is_ok_and(|grants| {
            grants
                .values()
                .any(|grant| grant.allocation == gone.allocation)
        });
        if !used
            && let Ok(bound) = self.bound.try_lock()
            && let Some(bound) = bound.as_ref()
        {
            bound.socket.forget_route(gone.allocation);
        }
    }

    pub(crate) fn relays(&self) -> bool {
        self.relay.lock().is_ok_and(|relay| relay.is_some())
    }

    /// Relays the other device of `pipe` through `bound`'s socket, and gives its stand-in.
    pub(super) async fn relay_route(&self, pipe: Uuid, bound: &Bound) -> Option<SocketAddr> {
        let grant = self.grants.lock().ok()?.get(&pipe).cloned()?;
        let server = self.relay_server().await?;
        Some(bound.socket.route(&grant, server))
    }

    /// The stand-in of the other device of `pipe`, once the relay holds this device's slot.
    pub(super) async fn through_relay(&self, pipe: Uuid, bound: &Bound) -> Option<SocketAddr> {
        let stand_in = self.relay_route(pipe, bound).await?;
        let until = tokio::time::Instant::now() + REBIND * 2;
        while !bound.socket.bound(stand_in)? {
            if tokio::time::Instant::now() >= until {
                return None;
            }
            tokio::time::sleep(REBIND / 10).await;
        }
        Some(stand_in)
    }

    async fn relay_server(&self) -> Option<SocketAddr> {
        let relay = self.relay.lock().ok()?.clone()?;
        let found = tokio::time::timeout(STUN_WAIT, tokio::net::lookup_host(relay.as_str()))
            .await
            .ok()?
            .ok()?
            .collect::<Vec<_>>();
        found
            .iter()
            .find(|address| address.is_ipv4())
            .or_else(|| found.first())
            .copied()
    }
}

impl Demux {
    /// Relays `grant`'s allocation through this socket, binding its slot until the socket or
    /// the route ends, and gives the stand-in of the device at the other end.
    fn route(&self, grant: &Grant, server: SocketAddr) -> SocketAddr {
        let Ok(mut routes) = self.0.relays.lock() else {
            return server;
        };
        let (stand_in, new) = routes.add(grant, server, self.0.dual);
        drop(routes);
        if new {
            tokio::spawn(keep_bound(
                Arc::downgrade(&self.0),
                stand_in,
                grant.clone(),
                server,
            ));
        }
        stand_in
    }

    fn forget_route(&self, allocation: [u8; ALLOCATION]) {
        if let Ok(mut routes) = self.0.relays.lock() {
            routes.remove(allocation);
        }
    }

    fn bound(&self, stand_in: SocketAddr) -> Option<bool> {
        self.0.relays.lock().ok()?.bound(stand_in)
    }
}

async fn keep_bound(socket: Weak<Shared>, stand_in: SocketAddr, grant: Grant, server: SocketAddr) {
    let mut binds = 0_u32;
    loop {
        let Some(shared) = socket.upgrade() else {
            return;
        };
        let Some(bound) = shared
            .relays
            .lock()
            .ok()
            .and_then(|routes| routes.bound(stand_in))
        else {
            return;
        };
        Demux(shared).send_to(server, &grant.bind());
        binds = binds.saturating_add(1);
        let quick = !bound && binds <= QUICK_BINDS;
        tokio::time::sleep(if quick { REBIND } else { KEEP_BOUND }).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(allocation: u8, slot: u8) -> Grant {
        Grant {
            allocation: [allocation; ALLOCATION],
            slot,
            secret: [7; SECRET],
            server_time: 1_000,
            given: tokio::time::Instant::now(),
        }
    }

    fn data(allocation: u8, payload: &[u8]) -> Vec<u8> {
        let mut header = [DATA; HEADER];
        header[1..].fill(allocation);
        wrapped(header, payload, None)
    }

    #[tokio::test]
    async fn a_batch_of_relayed_segments_hands_on_one_peer_at_a_time() {
        let server: SocketAddr = "203.0.113.5:3478".parse().unwrap();
        let mut routes = Routes::default();
        let (first, _) = routes.add(&grant(1, 0), server, true);
        let (second, _) = routes.add(&grant(2, 1), server, true);
        let mut batch = [
            data(1, b"aaaa"),
            data(1, b"bbbb"),
            data(2, b"cccc"),
            data(1, b"dd"),
        ]
        .concat();
        let mapped = SocketAddr::new(
            IpAddr::V6(Ipv4Addr::new(203, 0, 113, 5).to_ipv6_mapped()),
            3478,
        );
        let (handed, leftover) = routes.unwrap(&mut batch, mapped, HEADER + 4);
        assert_eq!(handed, Some((first, 8, 4)));
        assert_eq!(&batch[..8], b"aaaabbbb");
        let leftover = leftover.unwrap();
        let mut again = vec![0; 64];
        let (from, length, stride) = leftover.into_buffer(&mut again).unwrap();
        let (handed, leftover) = routes.unwrap(&mut again[..length], from, stride);
        assert_eq!(handed, Some((second, 4, 4)));
        assert_eq!(&again[..4], b"cccc");
        let mut last = vec![0; 64];
        let (from, length, stride) = leftover.unwrap().into_buffer(&mut last).unwrap();
        assert_eq!(
            routes.unwrap(&mut last[..length], from, stride).0,
            Some((first, 2, 4))
        );
        assert_eq!(&last[..2], b"dd");
        let (stranger, _) = routes.unwrap(
            &mut data(1, b"eeee"),
            "198.51.100.1:3478".parse().unwrap(),
            HEADER + 4,
        );
        assert_eq!(stranger, None);
    }

    #[test]
    fn a_bind_has_the_bytes_of_the_shared_vector() {
        let contract: Value = serde_json::from_str(
            &std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../protocol/terminal-connections.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let vector = &contract["relay"]["bindVector"];
        let bytes = |name: &str| -> Vec<u8> {
            let text = vector[name].as_str().unwrap();
            (0..text.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
                .collect()
        };
        let grant = Grant {
            allocation: bytes("allocationHex").try_into().unwrap(),
            slot: u8::try_from(vector["slot"].as_u64().unwrap()).unwrap(),
            secret: bytes("secretHex").try_into().unwrap(),
            server_time: 0,
            given: tokio::time::Instant::now(),
        };
        assert_eq!(
            grant.bind_at(vector["time"].as_i64().unwrap()).to_vec(),
            bytes("bindHex")
        );
    }

    #[tokio::test]
    async fn a_bound_answer_marks_only_the_slot_that_this_device_holds() {
        let server: SocketAddr = "203.0.113.5:3478".parse().unwrap();
        let mut routes = Routes::default();
        let (stand_in, _) = routes.add(&grant(3, 1), server, false);
        assert!(stand_in.is_ipv4());
        let other_slot = grant(3, 0).bind();
        let mut answer = other_slot[..=HEADER].to_vec();
        answer[0] = BOUND;
        routes.unwrap(&mut answer.clone(), server, answer.len());
        assert_eq!(routes.bound(stand_in), Some(false));
        answer[HEADER] = 1;
        routes.unwrap(&mut answer.clone(), server, answer.len());
        assert_eq!(routes.bound(stand_in), Some(true));
        let (relay, header) = routes.wrap(stand_in).unwrap();
        assert_eq!((relay, header), (server, [DATA, 3, 3, 3, 3, 3, 3, 3]));
    }
}
