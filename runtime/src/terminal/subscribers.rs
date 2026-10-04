use std::collections::HashMap;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::Checkpoint;
use crate::{Error, Result};

pub(super) const DATA_CAPACITY: usize = 512;
const CONTROL_CAPACITY: usize = 16;
const MAX_SUBSCRIBERS: usize = 32;
pub(super) const MAX_RAW_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct Subscription {
    pub connection_id: Uuid,
    pub incarnation_id: Uuid,
    pub checkpoint: Checkpoint,
    pub next_sequence: u64,
    pub data: mpsc::Receiver<DataFrame>,
    pub control: mpsc::Receiver<ControlFrame>,
    lifetime: CancellationToken,
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
