use std::{collections::VecDeque, time::Duration};

use uuid::Uuid;

use super::{DosiBody, DosiRecord, SharedInstructions, conversation::clip};

const MAX_PENDING: usize = 512;
const TEXT_BYTES: usize = 24 * 1024;
const REASONING_BYTES: usize = 8 * 1024;
const TOOL_BYTES: usize = 4 * 1024;
const FIRST_DELAY: Duration = Duration::from_secs(2);
const LONGEST_DELAY: Duration = Duration::from_mins(1);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    Idle,
    Running,
    Again,
    Wanted,
}

pub(super) struct Written {
    pub settled: Option<DosiRecord>,
    pub write: Option<DosiRecord>,
    pub sync: Option<u64>,
}

pub(super) struct Shared {
    pub user: Uuid,
    pub computer: String,
    synced: Option<u64>,
    heard: u64,
    reading: Reading,
    writing: bool,
    pending: VecDeque<DosiRecord>,
    saving: bool,
    unsaved: Option<SharedInstructions>,
    delay: Duration,
}

impl Shared {
    pub(super) fn new(user: Uuid, computer: String, synced: Option<u64>) -> Self {
        Self {
            user,
            computer,
            synced,
            heard: 0,
            reading: Reading::Idle,
            writing: false,
            pending: VecDeque::new(),
            saving: false,
            unsaved: None,
            delay: FIRST_DELAY,
        }
    }

    pub(super) const fn synced(&self) -> Option<u64> {
        self.synced
    }

    pub(super) fn push(&mut self, record: DosiRecord) -> Option<DosiRecord> {
        if self.pending.len() >= MAX_PENDING {
            tracing::warn!("Dosi has too many records waiting for the server");
            return None;
        }
        self.pending.push_back(fit(record));
        self.next_write()
    }

    fn next_write(&mut self) -> Option<DosiRecord> {
        if self.writing {
            return None;
        }
        let record = self.pending.front().cloned()?;
        self.writing = true;
        Some(record)
    }

    pub(super) fn written(&mut self, sequence: u64) -> Written {
        self.writing = false;
        self.delay = FIRST_DELAY;
        let record = self.pending.pop_front();
        let follows = self.synced.is_some_and(|synced| synced + 1 == sequence);
        if follows {
            self.synced = Some(sequence);
        }
        self.heard = self.heard.max(sequence);
        let write = self.next_write();
        Written {
            settled: record.filter(|_| follows),
            write,
            sync: self.behind().then(|| self.sync()).flatten(),
        }
    }

    fn behind(&self) -> bool {
        !self.writing && self.synced.is_some_and(|synced| self.heard > synced)
    }

    pub(super) fn hear(&mut self, sequence: Option<u64>) -> Option<u64> {
        let Some(sequence) = sequence else {
            return self.sync();
        };
        self.heard = self.heard.max(sequence);
        self.behind().then(|| self.sync()).flatten()
    }

    pub(super) fn write_failed(&mut self, retry: bool) -> Option<DosiRecord> {
        self.writing = false;
        if retry {
            return None;
        }
        self.pending.pop_front();
        self.next_write()
    }

    pub(super) fn sync(&mut self) -> Option<u64> {
        if matches!(self.reading, Reading::Running | Reading::Again) {
            self.reading = Reading::Again;
            return None;
        }
        self.reading = Reading::Running;
        Some(self.synced.unwrap_or_default())
    }

    pub(super) fn synced_to(&mut self, sequence: u64) -> bool {
        let again = self.reading == Reading::Again;
        self.reading = Reading::Idle;
        self.delay = FIRST_DELAY;
        self.synced = Some(self.synced.unwrap_or_default().max(sequence));
        again
    }

    pub(super) const fn sync_failed(&mut self) {
        self.reading = Reading::Wanted;
    }

    pub(super) const fn reset(&mut self) {
        self.synced = None;
        self.reading = Reading::Idle;
    }

    pub(super) fn save(&mut self, instructions: SharedInstructions) -> Option<SharedInstructions> {
        self.unsaved = Some(instructions);
        self.next_save()
    }

    fn next_save(&mut self) -> Option<SharedInstructions> {
        if self.saving {
            return None;
        }
        let instructions = self.unsaved.take()?;
        self.saving = true;
        Some(instructions)
    }

    pub(super) fn saved(
        &mut self,
        failed: Option<SharedInstructions>,
    ) -> Option<SharedInstructions> {
        self.saving = false;
        match failed {
            Some(instructions) => {
                self.unsaved.get_or_insert(instructions);
                None
            }
            None => self.next_save(),
        }
    }

    pub(super) const fn saving(&self) -> bool {
        self.saving || self.unsaved.is_some()
    }

    pub(super) fn retry(
        &mut self,
    ) -> (Option<DosiRecord>, Option<u64>, Option<SharedInstructions>) {
        let write = self.next_write();
        let wanted = self.reading == Reading::Wanted
            || (self.reading == Reading::Idle && (self.synced.is_none() || self.behind()));
        let sync = wanted.then(|| self.sync()).flatten();
        (write, sync, self.next_save())
    }

    pub(super) fn backoff(&mut self) -> Duration {
        let delay = self.delay;
        self.delay = (self.delay * 2).min(LONGEST_DELAY);
        delay
    }
}

fn fit(record: DosiRecord) -> DosiRecord {
    let DosiRecord::Item {
        conversation,
        mut item,
    } = record
    else {
        return record;
    };
    item.body = match item.body {
        DosiBody::Person { text } => DosiBody::Person {
            text: clip(&text, TEXT_BYTES),
        },
        DosiBody::Text { text, live } => DosiBody::Text {
            text: clip(&text, TEXT_BYTES),
            live,
        },
        DosiBody::Reasoning {
            text,
            live,
            seconds,
        } => DosiBody::Reasoning {
            text: clip(&text, REASONING_BYTES),
            live,
            seconds,
        },
        DosiBody::Tool {
            action,
            target,
            text,
            status,
            output,
        } => DosiBody::Tool {
            action,
            target,
            text: text.map(|text| clip(&text, TOOL_BYTES)),
            status,
            output: output.map(|output| clip(&output, TOOL_BYTES)),
        },
        DosiBody::Note { text } => DosiBody::Note {
            text: clip(&text, TEXT_BYTES),
        },
        body => body,
    };
    DosiRecord::Item { conversation, item }
}
