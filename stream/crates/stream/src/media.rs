//! Video frames as unreliable datagrams, one connection's flows apart by number.
//!
//! The host splits each frame into shards that fit one datagram, adds Reed-Solomon parity only
//! after the path lost packets lately, and sends a shard again only while its frame can still
//! arrive in time. The viewer puts frames back together, recovers lost shards from parity, hands
//! frames on in order, and reports which frames arrived, which shards it misses and which frames
//! it gave up on. A frame that the decoder can start from, a keyframe or an anchor, passes the
//! frames still missing before it.

use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

use bytes::{BufMut, Bytes, BytesMut};

use crate::viewer;

pub const SHARD: u8 = 0x01;
pub const REPORT: u8 = 0x02;
/// Kind, flow, frame, index, originals, recoveries and length.
pub const SHARD_HEADER: usize = 1 + 2 + 2 + 2 + 2 + 2 + 4;
const REPORT_HEADER: usize = 1 + 2 + 1;
const ENTRY_HEADER: usize = 2 + 1 + 2;
const COMPLETE: u8 = 0;
const LOST: u8 = 1;
const MISSING: u8 = 2;
/// The sender keeps frames this long for resends.
const KEPT: Duration = Duration::from_millis(250);
/// After a loss, frames carry parity until this long passes without one.
const LOSSY_FOR: Duration = Duration::from_millis(400);
const LEAST_PARITY: f64 = 0.1;
const MOST_PARITY: f64 = 0.5;
/// The least time a frame may take, which is two frames at 60 frames a second.
const LEAST_WAIT: Duration = Duration::from_millis(33);
const WAIT_MARGIN: Duration = Duration::from_millis(10);
/// How long the viewer waits after a frame's last shard before it asks for the ones it misses.
const QUIET: Duration = Duration::from_millis(3);
const MOST_SHARDS: usize = 32_768;
/// Frames that the viewer tracks at once; a gap wider than this means the stream started over.
const MOST_FRAMES: u64 = 512;

/// How long a frame may take from its first shard to the viewer before the viewer gives up on it:
/// long enough for one resend of a lost shard.
pub fn frame_wait(rtt: Duration) -> Duration {
    (rtt * 3 / 2 + WAIT_MARGIN).max(LEAST_WAIT)
}

/// The kind and flow of a datagram, which tell where it goes.
pub fn route(datagram: &[u8]) -> Option<(u8, u16)> {
    let kind = *datagram.first()?;
    let flow = u16::from_be_bytes(datagram.get(1..3)?.try_into().ok()?);
    Some((kind, flow))
}

/// A report without entries, which tells the host that the viewer takes the flow's datagrams.
pub fn greeting(flow: u16) -> Bytes {
    let mut report = BytesMut::with_capacity(REPORT_HEADER);
    report.put_u8(REPORT);
    report.put_u16(flow);
    report.put_u8(0);
    report.freeze()
}

/// The host's side of a flow.
pub struct Packer {
    flow: u16,
    next: u16,
    kept: VecDeque<Kept>,
    protection: Protection,
}

struct Kept {
    frame: u16,
    deadline: Instant,
    shards: Vec<Bytes>,
}

impl Packer {
    pub fn new(flow: u16) -> Self {
        Self {
            flow,
            next: 0,
            kept: VecDeque::new(),
            protection: Protection::default(),
        }
    }

    /// Splits a viewer message into datagrams of at most `datagram_bytes`, which the viewer must
    /// have by `deadline`.
    pub fn pack(
        &mut self,
        message: &[u8],
        datagram_bytes: usize,
        deadline: Instant,
        now: Instant,
    ) -> Vec<Bytes> {
        let frame = self.next;
        self.next = self.next.wrapping_add(1);
        let room = datagram_bytes.saturating_sub(SHARD_HEADER) & !1;
        let originals = message.len().div_ceil(room.max(2)).clamp(1, MOST_SHARDS);
        let parity = self.protection.parity(originals, now);
        let shard_bytes = (message.len().div_ceil(originals) + 1) & !1;
        let pieces: Vec<Vec<u8>> = (0..originals)
            .map(|index| {
                let start = (index * shard_bytes).min(message.len());
                let end = ((index + 1) * shard_bytes).min(message.len());
                let mut piece = message[start..end].to_vec();
                if parity > 0 {
                    piece.resize(shard_bytes, 0);
                }
                piece
            })
            .collect();
        let recoveries = if parity > 0 {
            reed_solomon_simd::encode(originals, parity, &pieces).unwrap_or_default()
        } else {
            Vec::new()
        };
        let count = |shards: usize| u16::try_from(shards).unwrap_or(u16::MAX);
        let header = Header {
            flow: self.flow,
            frame,
            index: 0,
            originals: count(originals),
            recoveries: count(recoveries.len()),
            length: u32::try_from(message.len()).unwrap_or(u32::MAX),
        };
        let shards: Vec<Bytes> = pieces
            .iter()
            .chain(&recoveries)
            .enumerate()
            .map(|(index, piece)| {
                Header {
                    index: count(index),
                    ..header
                }
                .shard(piece)
            })
            .collect();
        self.forget(now);
        self.kept.push_back(Kept {
            frame,
            deadline,
            shards: shards.clone(),
        });
        shards
    }

    /// Takes a viewer's report, and returns the shards to send again: those the viewer misses of
    /// frames that can still arrive in time.
    pub fn report(&mut self, report: &[u8], now: Instant, rtt: Duration) -> Vec<Bytes> {
        let mut again = Vec::new();
        for entry in entries(report) {
            let Some(position) = self.kept.iter().position(|kept| kept.frame == entry.frame) else {
                continue;
            };
            match entry.state {
                COMPLETE => {
                    self.protection.delivered(now);
                    self.kept.remove(position);
                }
                LOST => {
                    self.protection.lost(1.0, now);
                    self.kept.remove(position);
                }
                _ => {
                    let kept = &self.kept[position];
                    let total = kept.shards.len();
                    let missing = if entry.indices.is_empty() {
                        (0..total).collect()
                    } else {
                        entry.indices
                    };
                    #[expect(clippy::cast_precision_loss, reason = "shard counts are small")]
                    let fraction = missing.len() as f64 / total as f64;
                    self.protection.lost(fraction, now);
                    if now + rtt / 2 < kept.deadline {
                        again.extend(
                            missing
                                .into_iter()
                                .filter_map(|index| kept.shards.get(index).cloned()),
                        );
                    }
                }
            }
        }
        again
    }

    fn forget(&mut self, now: Instant) {
        while self
            .kept
            .front()
            .is_some_and(|kept| now.saturating_duration_since(kept.deadline) > KEPT)
        {
            self.kept.pop_front();
        }
    }
}

/// Parity only after losses, sized by how much the path lost, so a clean path carries none.
#[derive(Default)]
struct Protection {
    lossy_until: Option<Instant>,
    loss: f64,
}

impl Protection {
    fn parity(&self, originals: usize, now: Instant) -> usize {
        if self.lossy_until.is_none_or(|until| now >= until) {
            return 0;
        }
        let ratio = (2.0 * self.loss).clamp(LEAST_PARITY, MOST_PARITY);
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "shard counts are small"
        )]
        let parity = (originals as f64 * ratio).ceil() as usize;
        parity.clamp(1, MOST_SHARDS)
    }

    fn lost(&mut self, fraction: f64, now: Instant) {
        self.loss = 0.2f64.mul_add(fraction.clamp(0.0, 1.0), 0.8 * self.loss);
        self.lossy_until = Some(now + LOSSY_FOR);
    }

    fn delivered(&mut self, _now: Instant) {
        self.loss *= 0.98;
    }
}

#[derive(Clone, Copy)]
struct Header {
    flow: u16,
    frame: u16,
    index: u16,
    originals: u16,
    recoveries: u16,
    length: u32,
}

impl Header {
    fn shard(&self, piece: &[u8]) -> Bytes {
        let mut shard = BytesMut::with_capacity(SHARD_HEADER + piece.len());
        shard.put_u8(SHARD);
        shard.put_u16(self.flow);
        shard.put_u16(self.frame);
        shard.put_u16(self.index);
        shard.put_u16(self.originals);
        shard.put_u16(self.recoveries);
        shard.put_u32(self.length);
        shard.put_slice(piece);
        shard.freeze()
    }

    fn read(datagram: &[u8]) -> Option<(Self, &[u8])> {
        let field = |at: usize| -> Option<u16> {
            Some(u16::from_be_bytes(
                datagram.get(at..at + 2)?.try_into().ok()?,
            ))
        };
        if *datagram.first()? != SHARD {
            return None;
        }
        let header = Self {
            flow: field(1)?,
            frame: field(3)?,
            index: field(5)?,
            originals: field(7)?,
            recoveries: field(9)?,
            length: u32::from_be_bytes(datagram.get(11..15)?.try_into().ok()?),
        };
        let valid = header.originals > 0 && header.index < header.originals + header.recoveries;
        valid.then(|| (header, &datagram[SHARD_HEADER..]))
    }
}

/// The viewer's side of a flow.
pub struct Unpacker {
    flow: u16,
    frames: BTreeMap<u64, Assembly>,
    /// The next frame to hand on, and the newest seen, counting past the 16-bit wrap.
    next: Option<u64>,
    newest: Option<u64>,
    /// When the viewer last asked for frames of which no shard came.
    asked_absent: BTreeMap<u64, Instant>,
    told: Vec<(u64, u8, Vec<u16>)>,
}

struct Assembly {
    originals: usize,
    recoveries: usize,
    length: usize,
    shards: Vec<Option<Bytes>>,
    arrived: usize,
    first_at: Instant,
    last_at: Instant,
    asked_at: Option<Instant>,
    whole: Option<Bytes>,
}

impl Assembly {
    fn new(header: &Header, now: Instant) -> Self {
        Self {
            originals: usize::from(header.originals),
            recoveries: usize::from(header.recoveries),
            length: header.length as usize,
            shards: vec![None; usize::from(header.originals) + usize::from(header.recoveries)],
            arrived: 0,
            first_at: now,
            last_at: now,
            asked_at: None,
            whole: None,
        }
    }

    fn take(&mut self, index: usize, piece: &[u8], now: Instant) {
        let Some(slot) = self.shards.get_mut(index) else {
            return;
        };
        if slot.is_some() || self.whole.is_some() {
            return;
        }
        *slot = Some(Bytes::copy_from_slice(piece));
        self.arrived += 1;
        self.last_at = now;
        self.whole = self.assemble();
    }

    fn assemble(&self) -> Option<Bytes> {
        let originals = &self.shards[..self.originals];
        if originals.iter().all(Option::is_some) {
            return Some(self.joined(originals.iter().flatten().map(AsRef::as_ref)));
        }
        if self.recoveries == 0 || self.arrived < self.originals {
            return None;
        }
        let present = |range: std::ops::Range<usize>| {
            self.shards[range]
                .iter()
                .enumerate()
                .filter_map(|(index, shard)| shard.as_ref().map(|shard| (index, shard.as_ref())))
                .collect::<Vec<_>>()
        };
        let restored = reed_solomon_simd::decode(
            self.originals,
            self.recoveries,
            present(0..self.originals),
            present(self.originals..self.shards.len()),
        )
        .ok()?;
        let pieces = (0..self.originals).map(|index| {
            self.shards[index]
                .as_deref()
                .or_else(|| restored.get(&index).map(Vec::as_slice))
        });
        let pieces: Option<Vec<&[u8]>> = pieces.collect();
        Some(self.joined(pieces?.into_iter()))
    }

    fn joined<'a>(&self, pieces: impl Iterator<Item = &'a [u8]>) -> Bytes {
        let mut whole = BytesMut::with_capacity(self.length);
        for piece in pieces {
            whole.put_slice(piece);
        }
        whole.truncate(self.length);
        whole.freeze()
    }

    fn missing(&self) -> Vec<u16> {
        self.shards[..self.originals]
            .iter()
            .enumerate()
            .filter(|(_, shard)| shard.is_none())
            .map(|(index, _)| u16::try_from(index).unwrap_or(u16::MAX))
            .collect()
    }
}

impl Unpacker {
    pub fn new(flow: u16) -> Self {
        Self {
            flow,
            frames: BTreeMap::new(),
            next: None,
            newest: None,
            asked_absent: BTreeMap::new(),
            told: Vec::new(),
        }
    }

    /// Takes a shard, and returns the viewer messages it completes, in order.
    pub fn shard(&mut self, datagram: &[u8], now: Instant) -> Vec<Bytes> {
        let Some((header, piece)) = Header::read(datagram) else {
            return Vec::new();
        };
        if header.flow != self.flow {
            return Vec::new();
        }
        let frame = self.unwrap(header.frame);
        let next = *self.next.get_or_insert(frame);
        if frame < next {
            return Vec::new();
        }
        if frame - next >= MOST_FRAMES {
            self.frames.clear();
            self.next = Some(frame);
        }
        let assembly = self
            .frames
            .entry(frame)
            .or_insert_with(|| Assembly::new(&header, now));
        if assembly.originals != usize::from(header.originals)
            || assembly.recoveries != usize::from(header.recoveries)
        {
            return Vec::new();
        }
        assembly.take(usize::from(header.index), piece, now);
        if assembly.whole.is_some() {
            self.told.push((frame, COMPLETE, Vec::new()));
        }
        self.hand_on(now, None)
    }

    /// Gives up on frames that cannot arrive in time, asks for the shards that frames still miss,
    /// and returns the messages that can now go on, with a report for the host when there is news.
    pub fn tick(
        &mut self,
        now: Instant,
        rtt: Duration,
        room: usize,
    ) -> (Vec<Bytes>, Option<Bytes>) {
        let handed = self.hand_on(now, Some(rtt));
        let ask_again = (rtt / 2).max(QUIET);
        let newest = self.frames.keys().next_back().copied();
        for (&frame, assembly) in &mut self.frames {
            if assembly.whole.is_some() {
                continue;
            }
            let later = newest.is_some_and(|newest| newest > frame);
            let quiet = now.saturating_duration_since(assembly.last_at) >= QUIET;
            let asked = assembly
                .asked_at
                .is_some_and(|at| now.saturating_duration_since(at) < ask_again);
            if (later || quiet) && !asked {
                assembly.asked_at = Some(now);
                self.told.push((frame, MISSING, assembly.missing()));
            }
        }
        if let Some(next) = self.next {
            self.asked_absent = self.asked_absent.split_off(&next);
            for absent in next..newest.unwrap_or(next) {
                let asked = self
                    .asked_absent
                    .get(&absent)
                    .is_some_and(|at| now.saturating_duration_since(*at) < ask_again);
                if !self.frames.contains_key(&absent) && !asked {
                    self.asked_absent.insert(absent, now);
                    self.told.push((absent, MISSING, Vec::new()));
                }
            }
        }
        (handed, self.report(room))
    }

    /// Hands on whole frames in order, skips frames past their time, and lets a frame that the
    /// decoder can start from pass the frames still missing before it.
    fn hand_on(&mut self, now: Instant, rtt: Option<Duration>) -> Vec<Bytes> {
        let mut handed = Vec::new();
        let starts = self
            .frames
            .iter()
            .rev()
            .find(|(_, assembly)| {
                assembly
                    .whole
                    .as_ref()
                    .is_some_and(|whole| starts_over(whole))
            })
            .map(|(frame, _)| *frame);
        while let Some(next) = self.next {
            let Some(newest) = self.frames.keys().next_back().copied() else {
                break;
            };
            if next > newest {
                break;
            }
            let ready = self
                .frames
                .get(&next)
                .is_some_and(|assembly| assembly.whole.is_some());
            let late = rtt.is_some_and(|rtt| {
                let since = self
                    .frames
                    .range(next..)
                    .next()
                    .map(|(_, assembly)| assembly.first_at);
                since.is_some_and(|since| now.saturating_duration_since(since) > frame_wait(rtt))
            });
            if ready {
                if let Some(whole) = self
                    .frames
                    .remove(&next)
                    .and_then(|assembly| assembly.whole)
                {
                    handed.push(whole);
                }
                self.next = Some(next + 1);
            } else if let Some(starts) = starts.filter(|starts| *starts > next) {
                // Frames before a gap that a keyframe or anchor passes cannot be decoded.
                for skipped in next..starts {
                    let whole = self
                        .frames
                        .remove(&skipped)
                        .is_some_and(|assembly| assembly.whole.is_some());
                    if !whole {
                        self.told.push((skipped, LOST, Vec::new()));
                    }
                }
                self.next = Some(starts);
            } else if late {
                self.frames.remove(&next);
                self.told.push((next, LOST, Vec::new()));
                self.next = Some(next + 1);
            } else {
                break;
            }
        }
        handed
    }

    fn report(&mut self, room: usize) -> Option<Bytes> {
        if self.told.is_empty() {
            return None;
        }
        let mut report = BytesMut::with_capacity(room);
        report.put_u8(REPORT);
        report.put_u16(self.flow);
        report.put_u8(0);
        let mut count: u8 = 0;
        let mut taken = 0;
        for (frame, state, indices) in &self.told {
            if count == u8::MAX || report.len() + ENTRY_HEADER > room {
                break;
            }
            let fit = (room - report.len() - ENTRY_HEADER) / 2;
            let indices = &indices[..indices.len().min(fit)];
            #[expect(clippy::cast_possible_truncation, reason = "frames travel as 16 bits")]
            report.put_u16(*frame as u16);
            report.put_u8(*state);
            report.put_u16(u16::try_from(indices.len()).unwrap_or(u16::MAX));
            for index in indices {
                report.put_u16(*index);
            }
            count += 1;
            taken += 1;
        }
        self.told.drain(..taken);
        report[REPORT_HEADER - 1] = count;
        Some(report.freeze())
    }

    /// Counts frames past the 16-bit wrap, from the first frame seen, which starts high enough for
    /// shards of earlier frames to count below it.
    fn unwrap(&mut self, frame: u16) -> u64 {
        let Some(newest) = self.newest else {
            let first = (1 << 16) + u64::from(frame);
            self.newest = Some(first);
            return first;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the low 16 bits of a frame"
        )]
        let low = newest as u16;
        let ahead = frame.wrapping_sub(low);
        let unwrapped = if ahead < u16::MAX / 2 {
            newest + u64::from(ahead)
        } else {
            newest.saturating_sub(u64::from(low.wrapping_sub(frame)))
        };
        self.newest = Some(newest.max(unwrapped));
        unwrapped
    }
}

/// True for a keyframe or an anchor, which the decoder can start from.
fn starts_over(message: &[u8]) -> bool {
    message.first() == Some(&viewer::FRAME)
        && message
            .get(5)
            .is_some_and(|flags| flags & (viewer::KEY | viewer::ANCHOR) != 0)
}

struct Entry {
    frame: u16,
    state: u8,
    indices: Vec<usize>,
}

fn entries(report: &[u8]) -> Vec<Entry> {
    let mut entries = Vec::new();
    if report.first() != Some(&REPORT) || report.len() < REPORT_HEADER {
        return entries;
    }
    let count = report[REPORT_HEADER - 1];
    let mut rest = &report[REPORT_HEADER..];
    for _ in 0..count {
        let Some(head) = rest.get(..ENTRY_HEADER) else {
            break;
        };
        let frame = u16::from_be_bytes([head[0], head[1]]);
        let state = head[2];
        let listed = usize::from(u16::from_be_bytes([head[3], head[4]]));
        let Some(list) = rest.get(ENTRY_HEADER..ENTRY_HEADER + 2 * listed) else {
            break;
        };
        let indices = list
            .as_chunks::<2>()
            .0
            .iter()
            .map(|index| usize::from(u16::from_be_bytes(*index)))
            .collect();
        entries.push(Entry {
            frame,
            state,
            indices,
        });
        rest = &rest[ENTRY_HEADER + 2 * listed..];
    }
    entries
}

/// The encoder's bitrate from what the path carries, lowered while the sender's queue grows or
/// frames come out larger than the bitrate allows (`SCReAMv2` draft, section 4.4).
#[derive(Debug, Default)]
pub struct Target {
    rate_adjust_factor: f64,
    frame_size_dev: f64,
}

const RATE_ADJUST_GAIN: f64 = 1.0 / 16.0;
const FRAME_SIZE_DEV_ALPHA: f64 = 1.0 / 64.0;

impl Target {
    /// Records a frame of `bytes` whose oldest part waited `queued` in the sender, for an encoder
    /// that makes a frame every `period` at `bitrate`.
    pub fn frame(&mut self, bytes: usize, queued: Duration, period: Duration, bitrate: f64) {
        let period = period.as_secs_f64().max(0.001);
        let error = (queued.as_secs_f64() - period / 4.0) / period;
        self.rate_adjust_factor = error
            .mul_add(RATE_ADJUST_GAIN, self.rate_adjust_factor)
            .clamp(0.0, 0.5);
        let nominal = bitrate * period / 8.0;
        if nominal > 0.0 {
            #[expect(clippy::cast_precision_loss, reason = "frame sizes are small")]
            let deviation = ((bytes as f64 - nominal) / nominal).max(0.0);
            self.frame_size_dev = FRAME_SIZE_DEV_ALPHA
                .mul_add(
                    deviation,
                    (1.0 - FRAME_SIZE_DEV_ALPHA) * self.frame_size_dev,
                )
                .min(0.2);
        }
    }

    /// The bitrate for the encoder when the path carries `carried` bits a second.
    pub fn bitrate(&self, carried: u64) -> u64 {
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bitrates are finite and positive"
        )]
        let bitrate =
            (carried as f64 / (1.2 + self.rate_adjust_factor + self.frame_size_dev)) as u64;
        bitrate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATAGRAM: usize = 1162;
    const RTT: Duration = Duration::from_millis(20);
    const ROOM: usize = 1162;

    fn message(bytes: usize, key: bool) -> Vec<u8> {
        let mut message = vec![viewer::FRAME, 0, 0, 0, 1, u8::from(key), 0];
        message.extend((0..bytes).map(|byte| u8::try_from(byte % 251).unwrap()));
        message
    }

    #[test]
    fn a_frame_arrives_whole_from_its_shards() {
        let now = Instant::now();
        let mut packer = Packer::new(7);
        let mut unpacker = Unpacker::new(7);
        let sent = message(5000, true);
        let shards = packer.pack(&sent, DATAGRAM, now + RTT, now);
        assert!(shards.len() > 1 && shards.iter().all(|shard| shard.len() <= DATAGRAM));
        let handed: Vec<Bytes> = shards
            .iter()
            .flat_map(|shard| unpacker.shard(shard, now))
            .collect();
        assert_eq!(handed, vec![Bytes::from(sent)]);
    }

    #[test]
    fn after_a_loss_parity_restores_a_lost_shard_without_a_resend() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        let mut unpacker = Unpacker::new(1);
        let first = packer.pack(&message(4000, true), DATAGRAM, now + RTT, now);
        unpacker.shard(&first[0], now);
        let (_, report) = unpacker.tick(now + Duration::from_millis(5), RTT, ROOM);
        packer.report(&report.unwrap(), now + Duration::from_millis(6), RTT);

        let sent = message(4000, false);
        let shards = packer.pack(&sent, DATAGRAM, now + Duration::from_millis(40), now);
        let originals = sent.len().div_ceil(DATAGRAM - SHARD_HEADER);
        assert!(
            shards.len() > originals,
            "the frame carries parity after a loss"
        );
        let mut unpacker = Unpacker::new(1);
        let handed: Vec<Bytes> = shards
            .iter()
            .skip(1)
            .flat_map(|shard| unpacker.shard(shard, now))
            .collect();
        assert_eq!(handed, vec![Bytes::from(sent)]);
    }

    #[test]
    fn a_missing_shard_comes_again_while_its_frame_has_time() {
        let now = Instant::now();
        let mut packer = Packer::new(2);
        let mut unpacker = Unpacker::new(2);
        let sent = message(3000, true);
        let shards = packer.pack(&sent, DATAGRAM, now + Duration::from_millis(100), now);
        for shard in shards.iter().skip(1) {
            assert_eq!(unpacker.shard(shard, now), Vec::<Bytes>::new());
        }
        let (handed, report) = unpacker.tick(now + QUIET, RTT, ROOM);
        assert_eq!(handed, Vec::<Bytes>::new());
        let again = packer.report(&report.unwrap(), now + QUIET, RTT);
        assert_eq!(again, vec![shards[0].clone()]);
        assert_eq!(
            unpacker.shard(&again[0], now + RTT),
            vec![Bytes::from(sent)]
        );
    }

    #[test]
    fn a_frame_past_its_time_is_given_up_and_a_keyframe_passes_it() {
        let now = Instant::now();
        let mut packer = Packer::new(3);
        let mut unpacker = Unpacker::new(3);
        let stuck = packer.pack(&message(3000, true), DATAGRAM, now + RTT, now);
        unpacker.shard(&stuck[1], now);
        let delta = packer.pack(&message(100, false), DATAGRAM, now + RTT, now);
        assert!(
            unpacker.shard(&delta[0], now).is_empty(),
            "a delta waits for the frame before it"
        );
        let key = message(100, true);
        let starts = packer.pack(&key, DATAGRAM, now + RTT, now);
        assert_eq!(unpacker.shard(&starts[0], now), vec![Bytes::from(key)]);

        let late = packer.pack(&message(3000, false), DATAGRAM, now + RTT, now);
        unpacker.shard(&late[0], now);
        let after = message(100, false);
        let next = packer.pack(&after, DATAGRAM, now + RTT, now);
        unpacker.shard(&next[0], now);
        let (handed, report) = unpacker.tick(now + frame_wait(RTT) * 2, RTT, ROOM);
        assert_eq!(handed, vec![Bytes::from(after)]);
        assert!(
            entries(&report.unwrap())
                .iter()
                .any(|entry| entry.state == LOST)
        );
    }

    #[test]
    fn a_queue_in_the_sender_lowers_the_target() {
        let mut target = Target::default();
        let period = Duration::from_millis(16);
        let before = target.bitrate(10_000_000);
        for _ in 0..30 {
            target.frame(20_000, period, period, 8_000_000.0);
        }
        assert!(target.bitrate(10_000_000) < before);
    }
}
