use std::collections::{HashMap, VecDeque};

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::Checkpoint;
use crate::{Error, Result, network::CheckpointCut};

pub(super) const DATA_CAPACITY: usize = 512;
const CONTROL_CAPACITY: usize = 16;
const MAX_SUBSCRIBERS: usize = 32;
const MAX_PENDING_RESIZES: usize = 32;
pub(super) const MAX_RAW_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct DataFrame {
    pub sequence: u64,
    pub bytes: Bytes,
}

#[derive(Debug, Clone)]
pub enum ControlFrame {
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Snapshot {
        checkpoint: Checkpoint,
        next_sequence: u64,
    },
    Closed {
        reason: String,
        final_sequence: u64,
    },
}

#[derive(Debug)]
pub enum Frame {
    Data(DataFrame),
    Control(ControlFrame),
}

#[derive(Debug)]
pub struct Subscription {
    pub connection_id: Uuid,
    pub incarnation_id: Uuid,
    pub checkpoint: Checkpoint,
    pub next_sequence: u64,
    pub data: mpsc::Receiver<DataFrame>,
    pub control: mpsc::Receiver<ControlFrame>,
    lifetime: CancellationToken,
    resizes: VecDeque<(u16, u16, u64)>,
    closing: Option<(String, u64)>,
    refreshed: Option<Box<CheckpointCut>>,
}

impl Subscription {
    pub async fn recv(&mut self) -> Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.ready()? {
                return Ok(Some(frame));
            }
            tokio::select! {
                biased;
                control = self.control.recv(), if self.closing.is_none() => match control {
                    Some(ControlFrame::Resize { rows, cols, at_sequence }) => {
                        if self.resizes.len() >= MAX_PENDING_RESIZES
                            || self.resizes.back().is_some_and(|&(_, _, last)| last > at_sequence)
                        {
                            return Err(invalid("terminal resize queue exceeded its ordering bound"));
                        }
                        self.resizes.push_back((rows, cols, at_sequence));
                    }
                    Some(ControlFrame::Snapshot { checkpoint, next_sequence }) => {
                        if next_sequence < self.next_sequence {
                            return Err(invalid("terminal snapshot regressed"));
                        }
                        self.next_sequence = next_sequence;
                        self.resizes.clear();
                        self.refreshed = None;
                        return Ok(Some(Frame::Control(ControlFrame::Snapshot {
                            checkpoint,
                            next_sequence,
                        })));
                    }
                    Some(ControlFrame::Closed { reason, final_sequence }) => {
                        self.closing = Some((reason, final_sequence));
                    }
                    None => return Ok(None),
                },
                data = self.data.recv() => match data {
                    Some(frame) if frame.sequence < self.next_sequence => {}
                    Some(frame) if frame.sequence == self.next_sequence => {
                        self.next_sequence = self
                            .next_sequence
                            .checked_add(1)
                            .ok_or_else(|| invalid("terminal output sequence exhausted"))?;
                        return Ok(Some(Frame::Data(frame)));
                    }
                    Some(_) => return Err(invalid("terminal output needs a fresh snapshot")),
                    None => {
                        return Ok(self.closing.take().map(|(reason, final_sequence)| {
                            Frame::Control(ControlFrame::Closed { reason, final_sequence })
                        }));
                    }
                },
            }
        }
    }

    fn ready(&mut self) -> Result<Option<Frame>> {
        let next = self.next_sequence;
        if let Some(cut) = self.refreshed.take_if(|cut| cut.next_sequence <= next) {
            if cut.next_sequence < next {
                return Err(invalid("terminal refresh snapshot regressed"));
            }
            return Ok(Some(Frame::Control(ControlFrame::Snapshot {
                checkpoint: cut.checkpoint,
                next_sequence: cut.next_sequence,
            })));
        }
        if let Some((rows, cols, at_sequence)) = self.resizes.pop_front_if(|(_, _, at)| *at <= next)
        {
            if at_sequence != next {
                return Err(invalid("terminal resize arrived out of order"));
            }
            return Ok(Some(Frame::Control(ControlFrame::Resize {
                rows,
                cols,
                at_sequence,
            })));
        }
        match self.closing.take_if(|(_, last)| *last <= next) {
            Some((_, last)) if last < next => Err(invalid("terminal close boundary regressed")),
            Some((reason, final_sequence)) => Ok(Some(Frame::Control(ControlFrame::Closed {
                reason,
                final_sequence,
            }))),
            None => Ok(None),
        }
    }

    pub fn refresh(&mut self, cut: CheckpointCut) {
        self.refreshed = Some(Box::new(cut));
    }

    pub fn can_refresh(&self) -> bool {
        self.refreshed.is_none() && self.closing.is_none()
    }
}

fn invalid(reason: &str) -> Error {
    Error::Invalid(reason.to_owned())
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.lifetime.cancel();
    }
}

struct Subscriber {
    data: mpsc::Sender<DataFrame>,
    control: mpsc::Sender<ControlFrame>,
    lifetime: CancellationToken,
    behind: bool,
}

impl Subscriber {
    fn is_active(&self) -> bool {
        !self.lifetime.is_cancelled() && !self.data.is_closed() && !self.control.is_closed()
    }

    fn caught_up(&self) -> bool {
        self.behind && self.is_active() && self.data.capacity() == self.data.max_capacity()
    }

    fn deliver(&mut self, frame: ControlFrame) -> bool {
        let snapshot = matches!(frame, ControlFrame::Snapshot { .. });
        match self.control.try_send(frame) {
            Ok(()) => {
                if snapshot {
                    self.behind = false;
                }
                true
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.behind = true;
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.lifetime.cancel();
    }
}

#[derive(Default)]
pub(crate) struct Subscribers {
    entries: HashMap<Uuid, Subscriber>,
}

impl Subscribers {
    pub(crate) fn subscribe(
        &mut self,
        incarnation_id: Uuid,
        checkpoint: Checkpoint,
        next_sequence: u64,
    ) -> Result<Subscription> {
        self.prune();
        if self.entries.len() >= MAX_SUBSCRIBERS {
            return Err(Error::Busy);
        }
        let connection_id = Uuid::now_v7();
        let (data_tx, data) = mpsc::channel(DATA_CAPACITY);
        let (control_tx, control) = mpsc::channel(CONTROL_CAPACITY);
        let lifetime = CancellationToken::new();
        self.entries.insert(
            connection_id,
            Subscriber {
                data: data_tx,
                control: control_tx,
                lifetime: lifetime.clone(),
                behind: false,
            },
        );
        Ok(Subscription {
            connection_id,
            incarnation_id,
            checkpoint,
            next_sequence,
            data,
            control,
            lifetime,
            resizes: VecDeque::new(),
            closing: None,
            refreshed: None,
        })
    }

    pub(crate) fn authorization(&self, connection_id: Uuid) -> Result<CancellationToken> {
        self.entries
            .get(&connection_id)
            .filter(|entry| entry.is_active())
            .map(|entry| entry.lifetime.clone())
            .ok_or(Error::Stale)
    }

    pub(crate) fn contains(&self, connection_id: Uuid) -> bool {
        self.entries
            .get(&connection_id)
            .is_some_and(Subscriber::is_active)
    }

    pub(crate) fn remove(&mut self, connection_id: Uuid) {
        self.entries.remove(&connection_id);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(feature = "host")]
    pub(crate) fn has_active(&self) -> bool {
        self.entries.values().any(Subscriber::is_active)
    }

    pub(crate) fn prune(&mut self) {
        self.entries.retain(|_, entry| entry.is_active());
    }

    pub(crate) fn replay(&self, connection: Uuid, frames: &[DataFrame]) -> bool {
        let Some(entry) = self
            .entries
            .get(&connection)
            .filter(|entry| entry.is_active())
        else {
            return false;
        };
        if frames.len() > entry.data.capacity()
            || frames.iter().any(|frame| frame.bytes.len() > MAX_RAW_BYTES)
        {
            return false;
        }
        frames
            .iter()
            .all(|frame| entry.data.try_send(frame.clone()).is_ok())
    }

    pub(crate) fn data(&mut self, frame: &DataFrame) {
        self.entries.retain(|_, entry| {
            if !entry.behind
                && (frame.bytes.len() > MAX_RAW_BYTES
                    || entry.data.try_send(frame.clone()).is_err())
            {
                entry.behind = true;
            }
            entry.is_active()
        });
    }

    pub(crate) fn control(&mut self, frame: &ControlFrame) {
        self.entries.retain(|_, entry| {
            if entry.behind && matches!(frame, ControlFrame::Resize { .. }) {
                return entry.is_active();
            }
            entry.is_active() && entry.deliver(frame.clone())
        });
    }

    pub(crate) fn caught_up(&self) -> bool {
        self.entries.values().any(Subscriber::caught_up)
    }

    pub(crate) fn refresh(&mut self, checkpoint: &Checkpoint, next_sequence: u64) {
        self.entries.retain(|_, entry| {
            if !entry.caught_up() {
                return entry.is_active();
            }
            entry.deliver(ControlFrame::Snapshot {
                checkpoint: checkpoint.clone(),
                next_sequence,
            })
        });
    }

    pub(crate) fn close(&mut self, reason: &str, final_sequence: u64) {
        for (_, entry) in self.entries.drain() {
            drop(entry.control.try_send(ControlFrame::Closed {
                reason: reason.to_owned(),
                final_sequence,
            }));
        }
    }
}

#[cfg(test)]
#[path = "subscribers_tests.rs"]
mod tests;
