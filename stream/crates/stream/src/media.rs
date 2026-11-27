//! Video frames as unreliable datagrams, one connection's flows apart by number.
//!
//! The host splits each frame into shards that fit one datagram, adds Reed-Solomon parity only
//! after the path lost packets lately, and keeps the shards in order until the connection takes
//! them. A frame's time follows its size and the pacing of the path: the host drops the shards
//! that wait past it, and the shards of frames that a later keyframe or anchor passes. It sends a
//! shard again only when the viewer misses one that left, while its frame can still arrive in
//! time. The viewer puts frames back together, recovers lost shards from parity, hands frames on
//! in order, and reports which frames arrived, which shards it misses and which frames it gave up
//! on. A frame that the decoder can start from, a keyframe or an anchor, passes the frames still
//! missing before it. The host names each frame that the viewer cannot have, so that the stream
//! repairs the picture at once.

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
/// How long the viewer's report on a frame may take beyond a frame wait and a round trip after
/// the frame's last shard went, before the sender takes the frame as lost on the way.
const REPORT_MARGIN: Duration = Duration::from_millis(250);
/// After a loss, frames carry parity until this long passes without one.
const LOSSY_FOR: Duration = Duration::from_millis(400);
const LEAST_PARITY: f64 = 0.1;
const MOST_PARITY: f64 = 0.5;
/// The least wait for a frame's next shard, which is two frames at 60 frames a second.
const LEAST_WAIT: Duration = Duration::from_millis(33);
const WAIT_MARGIN: Duration = Duration::from_millis(10);
/// The longest a frame may take, however its shards come.
const LONGEST_FRAME: Duration = Duration::from_secs(4);
/// The least wait after a frame's last shard before the viewer asks for the ones it misses.
const QUIET: Duration = Duration::from_millis(3);
/// The viewer asks for missing shards once this many of the widest waits between the shards of a
/// frame passed without one, so that it does not ask for shards that the pacer still holds.
const QUIET_GAPS: u32 = 2;
const MOST_SHARDS: usize = 32_768;
const MOST_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// The memory that the frames a viewer puts together may take at once.
const MOST_HELD: usize = 32 * 1024 * 1024;
/// Missing shards that the viewer keeps to report for one frame, more than a report carries.
const MOST_LISTED: usize = 1024;
/// Frames that the viewer tracks at once; a gap wider than this means the stream started over.
const MOST_FRAMES: u64 = 512;
/// What QUIC adds to each shard in its packet, which the pacing spends too.
const PACKET_BYTES: usize = 40;

/// How long the viewer waits for a frame's next shard before it gives up on the frame: long
/// enough for one resend of a lost shard.
pub fn frame_wait(rtt: Duration) -> Duration {
    (rtt * 3 / 2 + WAIT_MARGIN).max(LEAST_WAIT)
}

/// The kind and flow of a datagram, which tell where it goes.
pub fn route(datagram: &[u8]) -> Option<(u8, u16)> {
    let kind = *datagram.first()?;
    let flow = u16::from_be_bytes(datagram.get(1..3)?.try_into().ok()?);
    Some((kind, flow))
}

/// The id in a frame message's header.
pub fn frame_id(message: &[u8]) -> Option<u32> {
    if message.first() != Some(&viewer::FRAME) {
        return None;
    }
    Some(u32::from_be_bytes(message.get(1..5)?.try_into().ok()?))
}

/// A report without entries, which tells the host that the viewer takes the flow's datagrams.
pub fn greeting(flow: u16) -> Bytes {
    let mut report = BytesMut::with_capacity(REPORT_HEADER);
    report.put_u8(REPORT);
    report.put_u16(flow);
    report.put_u8(0);
    report.freeze()
}

/// The connection under a flow, as the sender sees it when a frame comes.
#[derive(Debug, Clone, Copy)]
pub struct Link {
    pub rtt: Duration,
    /// The rate at which the flow's datagrams leave, in bits per second.
    pub pacing: u64,
    /// Bytes that the connection holds to send before the flow's own.
    pub ahead: usize,
}

/// The host's side of a flow: the shards that wait for the connection, and the frames that the
/// viewer has not yet reported on.
pub struct Packer {
    flow: u16,
    next: u16,
    /// In the order packed.
    frames: VecDeque<Packed>,
    /// Shards that the viewer misses, which go before any new one.
    again: VecDeque<Again>,
    protection: Protection,
    /// Stream frame ids of frames that the viewer cannot have.
    lost: Vec<u32>,
}

struct Packed {
    frame: u16,
    id: Option<u32>,
    starts_over: bool,
    queued_at: Instant,
    /// Shards that did not go by then never will.
    deadline: Instant,
    /// How long after its last shard went the viewer's report on the frame may take.
    answer: Duration,
    shards: Vec<Bytes>,
    /// The first `sent` shards went to the connection, each last at its time in `sent_at`.
    sent: usize,
    sent_at: Vec<Instant>,
    last_sent_at: Option<Instant>,
    recoveries: usize,
    rest: Rest,
    /// The stream heard of the frame's loss, or needs no word: a frame that passes it arrived.
    told: bool,
}

/// What becomes of the shards of a frame that did not go yet.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rest {
    Waits,
    /// Their time ended.
    Dropped,
    /// A later keyframe or anchor passes the frame.
    Passed,
}

impl Packed {
    /// The viewer's report on the frame comes before this, if the frame reached it.
    fn forget_at(&self) -> Instant {
        self.deadline
            .max(self.last_sent_at.unwrap_or(self.deadline))
            + self.answer
    }

    /// Whether a shard sent again now can arrive before the frame took the longest a frame may.
    /// The viewer asks only while it waits for the frame, which it does as long as shards come.
    fn worth_sending(&self, now: Instant, rtt: Duration) -> bool {
        self.sent_at
            .first()
            .is_some_and(|first| now + rtt / 2 < *first + LONGEST_FRAME)
    }

    fn unsent(&self) -> &[Bytes] {
        if self.rest == Rest::Waits {
            &self.shards[self.sent..]
        } else {
            &[]
        }
    }
}

struct Again {
    frame: u16,
    index: usize,
    asked_at: Instant,
    until: Instant,
    shard: Bytes,
}

impl Packer {
    pub fn new(flow: u16) -> Self {
        Self {
            flow,
            next: 0,
            frames: VecDeque::new(),
            again: VecDeque::new(),
            protection: Protection::default(),
            lost: Vec::new(),
        }
    }

    /// Splits a viewer message into shards of at most `datagram_bytes`, which wait for the
    /// connection until the time that the frame needs on `link`. A keyframe or an anchor makes
    /// the shards of earlier frames useless, since the viewer passes those frames.
    pub fn pack(&mut self, message: &[u8], datagram_bytes: usize, link: Link, now: Instant) {
        self.expire(now);
        let id = frame_id(message);
        if message.len() > MOST_FRAME_BYTES {
            self.lost.extend(id);
            return;
        }
        let starts_over = starts_over(message);
        if starts_over {
            for earlier in &mut self.frames {
                earlier.rest = Rest::Passed;
            }
            self.again.clear();
        }
        let frame = self.next;
        self.next = self.next.wrapping_add(1);
        let room = datagram_bytes.saturating_sub(SHARD_HEADER) & !1;
        let originals = message.len().div_ceil(room.max(2)).clamp(1, MOST_SHARDS);
        let parity = self
            .protection
            .parity(originals, now)
            .min(MOST_SHARDS - originals);
        let shard_bytes = part_bytes(message.len(), originals);
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
        let own: usize = shards.iter().map(|shard| shard.len() + PACKET_BYTES).sum();
        let crossing = crossing(self.waiting_bytes() + link.ahead + own, link.pacing);
        let deadline = now + (crossing + frame_wait(link.rtt)).min(LONGEST_FRAME);
        self.frames.push_back(Packed {
            frame,
            id,
            starts_over,
            queued_at: now,
            deadline,
            answer: frame_wait(link.rtt) + link.rtt + REPORT_MARGIN,
            shards,
            sent: 0,
            sent_at: Vec::new(),
            last_sent_at: None,
            recoveries: recoveries.len(),
            rest: Rest::Waits,
            told: false,
        });
    }

    /// The shard to send now, if one waits; [`Self::sent`] takes it once the connection did.
    pub fn next(&mut self, now: Instant) -> Option<Bytes> {
        self.expire(now);
        if let Some(again) = self.again.front() {
            return Some(again.shard.clone());
        }
        self.frames
            .iter()
            .find_map(|packed| packed.unsent().first().cloned())
    }

    /// Takes the shard that [`Self::next`] returned, which went to the connection.
    pub fn sent(&mut self, now: Instant) {
        if let Some(again) = self.again.pop_front() {
            if let Some(packed) = self
                .frames
                .iter_mut()
                .find(|packed| packed.frame == again.frame)
            {
                packed.sent_at[again.index] = now;
                packed.last_sent_at = Some(now);
            }
            return;
        }
        if let Some(packed) = self
            .frames
            .iter_mut()
            .find(|packed| !packed.unsent().is_empty())
        {
            packed.sent += 1;
            packed.sent_at.push(now);
            packed.last_sent_at = Some(now);
        }
    }

    /// When a rule acts next without news: a frame's or a resend's time ends, or a frame's
    /// report is overdue.
    pub fn wake(&self) -> Option<Instant> {
        let unsent = self
            .frames
            .iter()
            .filter(|packed| !packed.unsent().is_empty())
            .map(|packed| packed.deadline);
        let again = self.again.iter().map(|again| again.until);
        let forget = self.frames.iter().map(Packed::forget_at);
        unsent.chain(again).chain(forget).min()
    }

    /// How long the oldest shard that still waits has waited.
    pub fn waited(&self, now: Instant) -> Duration {
        let packed = self
            .frames
            .iter()
            .find(|packed| !packed.unsent().is_empty())
            .map(|packed| packed.queued_at);
        let again = self.again.front().map(|again| again.asked_at);
        packed
            .into_iter()
            .chain(again)
            .min()
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }

    /// Takes a viewer's report: frames that arrived or that it gave up on, and shards that it
    /// misses, which go again while the frame can still arrive. A shard that still waits here, or
    /// that left less than a round trip ago, may be on its way and is no loss.
    pub fn report(&mut self, report: &[u8], now: Instant, rtt: Duration) {
        for entry in entries(report) {
            let Some(position) = self
                .frames
                .iter()
                .position(|packed| packed.frame == entry.frame)
            else {
                continue;
            };
            match entry.state {
                COMPLETE => {
                    self.protection.delivered(now);
                    if self.frames[position].starts_over {
                        for passed in self.frames.range_mut(..position) {
                            passed.told = true;
                        }
                    }
                    self.settle(position);
                }
                LOST => {
                    let packed = &mut self.frames[position];
                    if packed.rest == Rest::Waits {
                        self.protection.lost(1.0, now);
                    }
                    if !std::mem::replace(&mut packed.told, true) {
                        self.lost.extend(packed.id);
                    }
                    self.settle(position);
                }
                _ => self.resend(position, &entry.indices, now, rtt),
            }
        }
    }

    /// Takes the stream frame ids of the frames that the viewer cannot have: their time ended
    /// before their shards went, the viewer gave up on them, or no report on them came. A frame
    /// that the viewer put together is never among them, however late its acknowledgement, nor
    /// one that a keyframe or anchor which arrived passes.
    pub fn lost(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.lost)
    }

    fn resend(&mut self, position: usize, indices: &[usize], now: Instant, rtt: Duration) {
        let packed = &self.frames[position];
        if packed.rest == Rest::Passed {
            return;
        }
        let gone = |index: &usize| {
            packed
                .sent_at
                .get(*index)
                .is_some_and(|at| now >= *at + rtt)
        };
        let missing: Vec<usize> = if indices.is_empty() {
            (0..packed.sent).filter(gone).collect()
        } else {
            indices.iter().copied().filter(gone).collect()
        };
        if missing.is_empty() {
            return;
        }
        #[expect(clippy::cast_precision_loss, reason = "shard counts are small")]
        let fraction = missing.len() as f64 / packed.shards.len() as f64;
        self.protection.lost(fraction, now);
        if packed.told || !packed.worth_sending(now, rtt) {
            return;
        }
        for index in missing {
            let waits = self
                .again
                .iter()
                .any(|again| again.frame == packed.frame && again.index == index);
            if !waits {
                self.again.push_back(Again {
                    frame: packed.frame,
                    index,
                    asked_at: now,
                    until: now + rtt,
                    shard: packed.shards[index].clone(),
                });
            }
        }
    }

    fn settle(&mut self, position: usize) {
        if let Some(settled) = self.frames.remove(position) {
            self.again.retain(|again| again.frame != settled.frame);
        }
    }

    fn expire(&mut self, now: Instant) {
        for packed in &mut self.frames {
            if packed.unsent().is_empty() || packed.deadline > now {
                continue;
            }
            packed.rest = Rest::Dropped;
            // Parity recovers as many shards as it carries.
            let dropped = packed.shards.len() - packed.sent;
            if dropped > packed.recoveries && !std::mem::replace(&mut packed.told, true) {
                self.lost.extend(packed.id);
            }
        }
        self.again.retain(|again| again.until > now);
        let lost = &mut self.lost;
        self.frames.retain(|packed| {
            let overdue = now >= packed.forget_at();
            if overdue && !packed.told {
                lost.extend(packed.id);
            }
            !overdue
        });
    }

    fn waiting_bytes(&self) -> usize {
        let unsent = self.frames.iter().flat_map(Packed::unsent);
        let again = self.again.iter().map(|again| &again.shard);
        unsent
            .chain(again)
            .map(|shard| shard.len() + PACKET_BYTES)
            .sum()
    }
}

/// The time that `bytes` take at `pacing` bits per second.
fn crossing(bytes: usize, pacing: u64) -> Duration {
    let bits = u64::try_from(bytes).unwrap_or(u64::MAX).saturating_mul(8);
    Duration::from_micros(bits.saturating_mul(1_000_000) / pacing.max(1))
}

/// The bytes of each part of a frame of `length` bytes in `originals` parts, an even number so
/// that Reed-Solomon parity covers it.
fn part_bytes(length: usize, originals: usize) -> usize {
    (length.div_ceil(originals.max(1)) + 1) & !1
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

    /// Reads a shard whose counts fit one another, its frame's length and the shard itself, as
    /// the packer makes them; the other device of a connection may send anything.
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
        let piece = &datagram[SHARD_HEADER..];
        let originals = usize::from(header.originals);
        let shards = originals + usize::from(header.recoveries);
        let length = usize::try_from(header.length).ok()?;
        let valid = originals > 0
            && shards <= MOST_SHARDS
            && usize::from(header.index) < shards
            && length <= MOST_FRAME_BYTES
            && originals <= length.div_ceil(2).max(1)
            && piece.len() <= part_bytes(length, originals);
        valid.then_some((header, piece))
    }

    fn length(&self) -> usize {
        usize::try_from(self.length).unwrap_or(usize::MAX)
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
    /// Bytes that the frames being put together take.
    held: usize,
    /// The widest wait between two shards of the frames lately, from the pacing and the path.
    spacing: Duration,
    rtt: Duration,
}

struct Assembly {
    originals: usize,
    recoveries: usize,
    length: usize,
    shards: Vec<Option<Bytes>>,
    bytes: usize,
    arrived: usize,
    first_at: Instant,
    last_at: Instant,
    /// The highest index that arrived, and the widest wait before a shard in the order sent,
    /// while no resend can be among them.
    highest: usize,
    widest: Duration,
    asked_at: Option<Instant>,
    whole: Option<Bytes>,
}

impl Assembly {
    fn new(header: &Header, now: Instant) -> Self {
        Self {
            originals: usize::from(header.originals),
            recoveries: usize::from(header.recoveries),
            length: header.length(),
            shards: vec![None; Self::slots(header)],
            bytes: 0,
            arrived: 0,
            first_at: now,
            last_at: now,
            highest: usize::from(header.index),
            widest: Duration::ZERO,
            asked_at: None,
            whole: None,
        }
    }

    fn slots(header: &Header) -> usize {
        usize::from(header.originals) + usize::from(header.recoveries)
    }

    /// What an assembly for `header` takes before any shard.
    fn cost(header: &Header) -> usize {
        Self::slots(header) * size_of::<Option<Bytes>>()
    }

    fn held(&self) -> usize {
        self.whole.as_ref().map_or_else(
            || self.shards.len() * size_of::<Option<Bytes>>() + self.bytes,
            Bytes::len,
        )
    }

    fn of(&self, header: &Header) -> bool {
        self.originals == usize::from(header.originals)
            && self.recoveries == usize::from(header.recoveries)
            && self.length == header.length()
    }

    fn take(&mut self, index: usize, piece: &[u8], now: Instant, rtt: Duration) {
        if self.whole.is_some() {
            return;
        }
        let Some(slot) = self.shards.get_mut(index) else {
            return;
        };
        if slot.is_some() {
            return;
        }
        *slot = Some(Bytes::copy_from_slice(piece));
        self.bytes += piece.len();
        let resent = self.asked_at.is_some_and(|at| now >= at + rtt);
        if self.arrived > 0 && index > self.highest && !resent {
            self.widest = self.widest.max(now.saturating_duration_since(self.last_at));
        }
        self.highest = self.highest.max(index);
        self.arrived += 1;
        self.last_at = now;
        self.whole = self.assemble();
        if self.whole.is_some() {
            self.shards = Vec::new();
            self.bytes = 0;
        }
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
            .take(MOST_LISTED)
            .collect()
    }

    /// How long no shard may come before the viewer asks for the missing ones.
    fn quiet(&self, spacing: Duration) -> Duration {
        (self.widest.max(spacing) * QUIET_GAPS).max(QUIET)
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
            held: 0,
            spacing: Duration::ZERO,
            rtt: Duration::ZERO,
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
            self.held = 0;
            self.next = Some(frame);
        }
        if !self.frames.contains_key(&frame) {
            let cost = Assembly::cost(&header);
            if self.held + cost > MOST_HELD {
                return Vec::new();
            }
            self.held += cost;
            self.frames.insert(frame, Assembly::new(&header, now));
        }
        let Some(assembly) = self.frames.get_mut(&frame) else {
            return Vec::new();
        };
        if !assembly.of(&header) || self.held + piece.len() > MOST_HELD {
            return Vec::new();
        }
        let (held, whole) = (assembly.held(), assembly.whole.is_some());
        assembly.take(usize::from(header.index), piece, now, self.rtt);
        self.held = self.held - held + assembly.held();
        if !whole && assembly.whole.is_some() {
            self.told.push((frame, COMPLETE, Vec::new()));
            if assembly.arrived > 1 {
                self.spacing = assembly.widest.max(self.spacing * 7 / 8);
            }
        }
        self.hand_on(now, None)
    }

    /// Whether a tick has nothing to do until the next shard comes.
    pub fn idle(&self) -> bool {
        self.frames.is_empty() && self.told.is_empty()
    }

    /// Gives up on frames that cannot arrive in time, asks for the shards that frames still miss,
    /// and returns the messages that can now go on, with a report for the host when there is news.
    pub fn tick(
        &mut self,
        now: Instant,
        rtt: Duration,
        room: usize,
    ) -> (Vec<Bytes>, Option<Bytes>) {
        self.rtt = rtt;
        let handed = self.hand_on(now, Some(rtt));
        let newest = self.frames.keys().next_back().copied();
        for (&frame, assembly) in &mut self.frames {
            if assembly.whole.is_some() {
                continue;
            }
            let quiet = assembly.quiet(self.spacing);
            let later = newest.is_some_and(|newest| newest > frame);
            let still = now.saturating_duration_since(assembly.last_at) >= quiet;
            let asked = assembly
                .asked_at
                .is_some_and(|at| now.saturating_duration_since(at) < (rtt / 2).max(quiet));
            if (later || still) && !asked {
                assembly.asked_at = Some(now);
                self.told
                    .retain(|(told, state, _)| *told != frame || *state != MISSING);
                self.told.push((frame, MISSING, assembly.missing()));
            }
        }
        if let Some(next) = self.next {
            let ask_again = (rtt / 2).max(QUIET);
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

    /// Hands on whole frames in order, gives up on a frame once no shard of it came for its wait
    /// or it took the longest a frame may, and lets a frame that the decoder can start from pass
    /// the frames still missing before it.
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
                let since = |at: Instant| now.saturating_duration_since(at);
                match self.frames.get(&next) {
                    Some(assembly) => {
                        since(assembly.last_at) > frame_wait(rtt)
                            || since(assembly.first_at) > LONGEST_FRAME
                    }
                    // No shard of it came: from the first shard of a later frame on.
                    None => self
                        .frames
                        .range(next..)
                        .next()
                        .is_some_and(|(_, later)| since(later.first_at) > frame_wait(rtt)),
                }
            });
            if ready {
                if let Some(whole) = self.remove(next).and_then(|assembly| assembly.whole) {
                    handed.push(whole);
                }
                self.next = Some(next + 1);
            } else if let Some(starts) = starts.filter(|starts| *starts > next) {
                // Frames before a gap that a keyframe or anchor passes cannot be decoded.
                for skipped in next..starts {
                    let whole = self
                        .remove(skipped)
                        .is_some_and(|assembly| assembly.whole.is_some());
                    if !whole {
                        self.told.push((skipped, LOST, Vec::new()));
                    }
                }
                self.next = Some(starts);
            } else if late {
                self.remove(next);
                self.told.push((next, LOST, Vec::new()));
                self.next = Some(next + 1);
            } else {
                break;
            }
        }
        handed
    }

    fn remove(&mut self, frame: u64) -> Option<Assembly> {
        let assembly = self.frames.remove(&frame)?;
        self.held -= assembly.held();
        Some(assembly)
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
    const FAST: Link = Link {
        rtt: RTT,
        pacing: 100_000_000,
        ahead: 0,
    };

    fn message(id: u32, bytes: usize, key: bool) -> Vec<u8> {
        let mut message = vec![viewer::FRAME];
        message.extend(id.to_be_bytes());
        message.extend([u8::from(key), 0]);
        message.extend((0..bytes).map(|byte| u8::try_from(byte % 251).unwrap()));
        message
    }

    fn after(now: Instant, millis: u64) -> Instant {
        now + Duration::from_millis(millis)
    }

    /// The shards that the connection takes at once.
    fn drain(packer: &mut Packer, now: Instant) -> Vec<Bytes> {
        let mut shards = Vec::new();
        while let Some(shard) = packer.next(now) {
            packer.sent(now);
            shards.push(shard);
        }
        shards
    }

    fn pack(packer: &mut Packer, message: &[u8], now: Instant) -> Vec<Bytes> {
        packer.pack(message, DATAGRAM, FAST, now);
        drain(packer, now)
    }

    fn states(report: &[u8], state: u8) -> Vec<u16> {
        entries(report)
            .iter()
            .filter(|entry| entry.state == state)
            .map(|entry| entry.frame)
            .collect()
    }

    fn report(frame: u16, state: u8, indices: &[u16]) -> Vec<u8> {
        let mut report = vec![REPORT, 0, 1, 1];
        report.extend(frame.to_be_bytes());
        report.push(state);
        report.extend(u16::try_from(indices.len()).unwrap().to_be_bytes());
        for index in indices {
            report.extend(index.to_be_bytes());
        }
        report
    }

    #[test]
    fn a_frame_arrives_whole_from_its_shards() {
        let now = Instant::now();
        let mut packer = Packer::new(7);
        let mut unpacker = Unpacker::new(7);
        let sent = message(1, 5000, true);
        let shards = pack(&mut packer, &sent, now);
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
        let first = pack(&mut packer, &message(1, 4000, true), now);
        unpacker.shard(&first[0], now);
        let (_, report) = unpacker.tick(after(now, 5), RTT, ROOM);
        packer.report(&report.unwrap(), after(now, 45), RTT);
        drain(&mut packer, after(now, 45));

        let sent = message(2, 4000, false);
        let shards = pack(&mut packer, &sent, after(now, 46));
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
    fn an_unpacker_rests_only_once_its_frames_are_handed_on_and_reported() {
        let now = Instant::now();
        let mut packer = Packer::new(4);
        let mut unpacker = Unpacker::new(4);
        assert!(unpacker.idle());
        let shards = pack(&mut packer, &message(1, 3000, true), now);
        unpacker.shard(&shards[0], now);
        assert!(!unpacker.idle(), "a frame being put together needs ticks");
        for shard in &shards[1..] {
            unpacker.shard(shard, now);
        }
        assert!(
            !unpacker.idle(),
            "the host has not heard that the frame came"
        );
        let (_, report) = unpacker.tick(now + QUIET, RTT, ROOM);
        assert!(report.is_some());
        assert!(unpacker.idle());
    }

    #[test]
    fn a_missing_shard_comes_again_while_its_frame_has_time() {
        let now = Instant::now();
        let mut packer = Packer::new(2);
        let mut unpacker = Unpacker::new(2);
        let sent = message(1, 3000, true);
        let shards = pack(&mut packer, &sent, now);
        for shard in shards.iter().skip(1) {
            assert_eq!(unpacker.shard(shard, now), Vec::<Bytes>::new());
        }
        let (handed, report) = unpacker.tick(now + QUIET, RTT, ROOM);
        assert_eq!(handed, Vec::<Bytes>::new());
        packer.report(&report.unwrap(), now + RTT, RTT);
        let again = drain(&mut packer, now + RTT);
        assert_eq!(again, vec![shards[0].clone()]);
        assert_eq!(
            unpacker.shard(&again[0], now + RTT),
            vec![Bytes::from(sent)]
        );
    }

    #[test]
    fn a_frame_that_stops_coming_is_given_up_and_a_keyframe_passes_it() {
        let now = Instant::now();
        let mut packer = Packer::new(3);
        let mut unpacker = Unpacker::new(3);
        let stuck = pack(&mut packer, &message(1, 3000, true), now);
        unpacker.shard(&stuck[1], now);
        let delta = pack(&mut packer, &message(2, 100, false), now);
        assert!(
            unpacker.shard(&delta[0], now).is_empty(),
            "a delta waits for the frame before it"
        );
        let key = message(3, 100, true);
        let starts = pack(&mut packer, &key, now);
        assert_eq!(unpacker.shard(&starts[0], now), vec![Bytes::from(key)]);

        let late = pack(&mut packer, &message(4, 3000, false), now);
        unpacker.shard(&late[0], now);
        let after = message(5, 100, false);
        let next = pack(&mut packer, &after, now);
        unpacker.shard(&next[0], now);
        let (handed, report) = unpacker.tick(now + frame_wait(RTT) * 2, RTT, ROOM);
        assert_eq!(handed, vec![Bytes::from(after)]);
        assert_ne!(states(&report.unwrap(), LOST), Vec::<u16>::new());
    }

    /// A whole new screen on a slow link takes longer than any fixed limit: its time follows its
    /// size at the pacing, behind what waits before it, and the viewer waits while it comes.
    #[test]
    fn a_large_frame_on_a_slow_link_arrives_behind_the_frames_before_it() {
        let start = Instant::now();
        let mut packer = Packer::new(1);
        let mut unpacker = Unpacker::new(1);
        let link = Link {
            rtt: RTT,
            pacing: 8_000_000,
            ahead: 0,
        };
        let screen = message(1, 150_000, true);
        let typed = message(2, 1500, false);
        packer.pack(&screen, DATAGRAM, link, start);
        packer.pack(&typed, DATAGRAM, link, start);
        let one_way = RTT / 2;
        let mut now = start;
        let mut handed = Vec::new();
        let mut ticked = start;
        let mut lost = Vec::new();
        while now < start + Duration::from_secs(1) {
            if let Some(shard) = packer.next(now) {
                packer.sent(now);
                handed.extend(unpacker.shard(&shard, now + one_way));
                now += crossing(shard.len() + PACKET_BYTES, link.pacing);
            } else {
                now += Duration::from_millis(1);
            }
            if now.duration_since(ticked) >= Duration::from_millis(5) {
                ticked = now;
                let (more, report) = unpacker.tick(now + one_way, RTT, ROOM);
                handed.extend(more);
                if let Some(report) = report {
                    lost.extend(states(&report, LOST));
                    packer.report(&report, now, RTT);
                }
            }
        }
        assert_eq!(handed, vec![Bytes::from(screen), Bytes::from(typed)]);
        assert_eq!(lost, Vec::<u16>::new());
        assert_eq!(packer.lost(), Vec::<u32>::new());
    }

    #[test]
    fn shards_that_wait_past_their_frames_time_go_and_the_stream_hears_of_it_once() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        let link = Link {
            rtt: RTT,
            pacing: 1_000_000,
            ahead: 0,
        };
        packer.pack(&message(9, 3000, false), DATAGRAM, link, now);
        assert!(packer.next(now).is_some());
        packer.sent(now);
        let due = packer.wake().unwrap();
        assert!(
            due > now + frame_wait(RTT),
            "the frame's size gives it time"
        );
        let before = due.checked_sub(Duration::from_millis(1)).unwrap();
        assert!(packer.next(before).is_some());
        assert_eq!(packer.lost(), Vec::<u32>::new());
        assert_eq!(packer.next(due), None);
        assert_eq!(packer.lost(), [9]);
        packer.report(&report(0, LOST, &[]), due, RTT);
        assert_eq!(packer.lost(), Vec::<u32>::new());
    }

    #[test]
    fn a_frame_that_the_viewer_gave_up_on_is_told_to_the_stream_once() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        pack(&mut packer, &message(4, 3000, false), now);
        packer.report(&report(0, LOST, &[]), after(now, 40), RTT);
        packer.report(&report(0, LOST, &[]), after(now, 41), RTT);
        assert_eq!(packer.lost(), [4]);
    }

    #[test]
    fn a_keyframe_drops_the_shards_of_earlier_frames_that_did_not_go_and_they_need_no_word() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        packer.pack(&message(1, 30_000, false), DATAGRAM, FAST, now);
        let first = packer.next(now).unwrap();
        packer.sent(now);
        let key = message(2, 3000, true);
        packer.pack(&key, DATAGRAM, FAST, now);
        let shards = drain(&mut packer, now);
        let mut unpacker = Unpacker::new(1);
        unpacker.shard(&first, now);
        let handed: Vec<Bytes> = shards
            .iter()
            .flat_map(|shard| unpacker.shard(shard, now))
            .collect();
        assert_eq!(handed, vec![Bytes::from(key)]);
        let (_, told) = unpacker.tick(after(now, 1), RTT, ROOM);
        let told = told.unwrap();
        assert_eq!(states(&told, LOST), [0]);
        packer.report(&told, after(now, 2), RTT);
        packer.report(&report(0, MISSING, &[1, 2]), after(now, 2), RTT);
        assert_eq!(drain(&mut packer, after(now, 2)), Vec::<Bytes>::new());
        assert_eq!(packer.lost(), Vec::<u32>::new());
        let next = pack(&mut packer, &message(3, 3000, false), after(now, 3));
        assert_eq!(next.len(), 3, "a passed frame is no loss, so no parity");
    }

    /// On a slow link the viewer may ask for shards that the pacer has not sent yet, or that left
    /// less than a round trip ago. They are on their way: they go once, and the ask turns no
    /// parity on.
    #[test]
    fn a_shard_that_still_waits_is_not_sent_again_and_its_ask_is_no_loss() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        packer.pack(&message(1, 11_000, false), DATAGRAM, FAST, now);
        for _ in 0..3 {
            packer.next(now).unwrap();
            packer.sent(now);
        }
        let waiting = report(0, MISSING, &[3, 4, 5, 6, 7, 8, 9]);
        packer.report(&waiting, now, RTT);
        assert_eq!(drain(&mut packer, now).len(), 7);
        packer.report(&waiting, after(now, 5), RTT);
        assert_eq!(drain(&mut packer, after(now, 5)), Vec::<Bytes>::new());
        packer.report(&report(0, COMPLETE, &[]), after(now, 6), RTT);
        let next = pack(&mut packer, &message(2, 3000, false), after(now, 7));
        assert_eq!(next.len(), 3, "no parity");

        packer.report(&report(1, MISSING, &[1]), after(now, 30), RTT);
        packer.report(&report(1, MISSING, &[1]), after(now, 31), RTT);
        assert_eq!(drain(&mut packer, after(now, 31)), vec![next[1].clone()]);
    }

    /// The tail case: a frame lost whole on the way, with no frame after it, leaves the viewer
    /// nothing to report.
    #[test]
    fn a_frame_without_any_report_is_told_lost_once_its_report_is_overdue_and_its_shards_go() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        pack(&mut packer, &message(6, 3000, false), now);
        packer.report(&report(0, MISSING, &[1]), after(now, 15), RTT);
        drain(&mut packer, after(now, 15));
        let overdue = packer.wake().unwrap();
        assert!(overdue > after(now, 40) + frame_wait(RTT) + RTT);
        packer.next(overdue.checked_sub(Duration::from_millis(1)).unwrap());
        assert_eq!(packer.lost(), Vec::<u32>::new());
        packer.next(overdue);
        assert_eq!(packer.lost(), [6]);
        assert!(packer.frames.is_empty());
        assert_eq!(packer.wake(), None);
    }

    /// A slow viewer acknowledges a frame late; the transport saw it arrive, so it is late,
    /// not lost.
    #[test]
    fn a_frame_that_the_viewer_put_together_is_never_told_lost() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        pack(&mut packer, &message(6, 3000, false), now);
        packer.report(&report(0, COMPLETE, &[]), after(now, 20), RTT);
        packer.next(after(now, 5000));
        assert_eq!(packer.lost(), Vec::<u32>::new());
    }

    #[test]
    fn frames_that_a_keyframe_passed_are_told_lost_only_when_the_keyframe_does_not_arrive() {
        let now = Instant::now();
        for arrives in [true, false] {
            let mut packer = Packer::new(1);
            pack(&mut packer, &message(1, 3000, false), now);
            pack(&mut packer, &message(2, 3000, true), now);
            packer.report(
                &report(1, if arrives { COMPLETE } else { LOST }, &[]),
                after(now, 30),
                RTT,
            );
            packer.report(&report(0, LOST, &[]), after(now, 31), RTT);
            let told = packer.lost();
            assert_eq!(told, if arrives { vec![] } else { vec![2, 1] }, "{arrives}");
        }
    }

    fn shards(packer: &mut Packer, id: u32, bytes: usize, now: Instant) -> Vec<Bytes> {
        pack(packer, &message(id, bytes, false), now)
    }

    #[test]
    fn the_viewer_waits_for_a_frame_while_its_shards_keep_coming() {
        let start = Instant::now();
        let mut packer = Packer::new(1);
        let mut unpacker = Unpacker::new(1);
        let sent = shards(&mut packer, 1, 20 * 1100, start);
        let every = frame_wait(RTT) * 3 / 4;
        let mut handed = Vec::new();
        let mut now = start;
        for shard in &sent {
            handed.extend(unpacker.shard(shard, now));
            for _ in 0..3 {
                now += every / 3;
                let (more, report) = unpacker.tick(now, RTT, ROOM);
                handed.extend(more);
                assert!(report.is_none_or(|report| states(&report, LOST).is_empty()));
            }
        }
        assert_eq!(handed.len(), 1);
    }

    #[test]
    fn the_viewer_gives_up_on_a_frame_once_no_shard_of_it_came_for_its_wait() {
        let now = Instant::now();
        let mut packer = Packer::new(1);
        let mut unpacker = Unpacker::new(1);
        let sent = shards(&mut packer, 1, 20 * 1100, now);
        for (index, shard) in sent.iter().take(10).enumerate() {
            unpacker.shard(shard, after(now, 20 * u64::try_from(index).unwrap()));
        }
        let last = after(now, 180);
        let (_, report) = unpacker.tick(last + frame_wait(RTT), RTT, ROOM);
        assert!(report.is_none_or(|report| states(&report, LOST).is_empty()));
        let (_, report) =
            unpacker.tick(last + frame_wait(RTT) + Duration::from_millis(1), RTT, ROOM);
        assert_eq!(states(&report.unwrap(), LOST), [0]);
    }

    #[test]
    fn a_drip_of_shards_cannot_hold_a_frame_past_the_longest_time() {
        let start = Instant::now();
        let mut packer = Packer::new(1);
        let mut unpacker = Unpacker::new(1);
        let sent = shards(&mut packer, 1, 400 * 1100, start);
        let mut gave_up = None;
        for (index, shard) in sent.iter().enumerate() {
            let now = start + Duration::from_millis(20) * u32::try_from(index).unwrap();
            unpacker.shard(shard, now);
            let (_, report) = unpacker.tick(now, RTT, ROOM);
            if report.is_some_and(|report| !states(&report, LOST).is_empty()) {
                gave_up = Some(now.duration_since(start));
                break;
            }
        }
        let gave_up = gave_up.unwrap();
        assert!(
            (LONGEST_FRAME..LONGEST_FRAME + Duration::from_millis(40)).contains(&gave_up),
            "{gave_up:?}"
        );
    }

    /// The pacer sends a frame in bursts. The viewer learns how far apart they come and does not
    /// ask for the shards of the next burst, only for those that stopped coming.
    #[test]
    fn the_viewer_asks_for_missing_shards_only_after_the_spacing_that_it_saw() {
        let start = Instant::now();
        let mut packer = Packer::new(1);
        let mut unpacker = Unpacker::new(1);
        let burst = Duration::from_millis(12);
        let mut now = start;
        let mut asked = Vec::new();
        for id in 1..=2 {
            let sent = shards(&mut packer, id, 16 * 1100, now);
            for (index, shard) in sent.iter().enumerate() {
                if index > 0 && index % 4 == 0 {
                    for _ in 0..2 {
                        now += burst / 2;
                        let (_, report) = unpacker.tick(now, RTT, ROOM);
                        if id == 2 {
                            asked.extend(
                                report.map_or_else(Vec::new, |report| states(&report, MISSING)),
                            );
                        }
                    }
                }
                if id == 2 && index == 13 {
                    break;
                }
                unpacker.shard(shard, now);
                now += Duration::from_micros(100);
            }
        }
        assert_eq!(asked, Vec::<u16>::new());
        let (_, report) = unpacker.tick(now + burst * 3, RTT, ROOM);
        assert_eq!(states(&report.unwrap(), MISSING), [1]);
    }

    fn crafted(frame: u16, originals: u16, recoveries: u16, length: u32, body: usize) -> Vec<u8> {
        let mut datagram = vec![SHARD];
        datagram.extend(1u16.to_be_bytes());
        datagram.extend(frame.to_be_bytes());
        datagram.extend(0u16.to_be_bytes());
        datagram.extend(originals.to_be_bytes());
        datagram.extend(recoveries.to_be_bytes());
        datagram.extend(length.to_be_bytes());
        datagram.extend(std::iter::repeat_n(0u8, body));
        datagram
    }

    /// The other device of a connection, which may be a friend's computer for the shared browser,
    /// can send any header.
    #[test]
    fn a_crafted_shard_neither_ends_the_viewer_nor_takes_its_memory() {
        let now = Instant::now();
        let mut unpacker = Unpacker::new(1);
        let overflowing = crafted(0, 40_000, 40_000, 100, 100);
        assert_eq!(unpacker.shard(&overflowing, now), Vec::<Bytes>::new());
        for (frame, originals, recoveries, length, body) in [
            (1, 32_767, 32_767, 100, 2),
            (2, 2, 0, 100, 100),
            (3, 1, 0, u32::MAX, 2),
        ] {
            unpacker.shard(&crafted(frame, originals, recoveries, length, body), now);
        }
        assert_eq!(unpacker.held, 0);
        for frame in 10..500 {
            unpacker.shard(&crafted(frame, 16_000, 16_000, 8_000_000, 2), now);
        }
        assert!(unpacker.held <= MOST_HELD, "{}", unpacker.held);
        assert!(unpacker.frames.len() < 40);
    }
}
