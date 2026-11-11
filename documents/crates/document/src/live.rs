use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use serde_json::{Value, json};

use crate::{
    contract::{
        self, Catchup, Changed, ClosedReason, Frame, Hash, Peer, RefusedReason, Rev, Seq, ToClient,
        ToDocument, Viewer,
    },
    format::{Assets, Format, Ordered, Refused, Unreadable},
};

const MOST_ASSET_BYTES: usize = 128 * 1024 * 1024;
const MOST_SAVED_STATES: usize = 32;

pub type PipeId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A pipe opened for this viewer. Its first message must be `join`.
    Connected {
        pipe: PipeId,
        viewer: Viewer,
    },
    Message {
        pipe: PipeId,
        frame: Vec<u8>,
    },
    Left {
        pipe: PipeId,
    },
    FileChanged {
        file: Vec<u8>,
        writer: String,
    },
    /// `file` is the file as it is on disk.
    Save {
        file: Vec<u8>,
    },
    Saved {
        rev: Rev,
    },
    /// Another program holds the file, and the save goes on trying.
    SaveWaits,
    /// The journal starts again from this file.
    Rebased {
        file: Vec<u8>,
        epoch: u64,
    },
    Closing {
        reason: ClosedReason,
    },
}

/// What a document asks its host to do, in this order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `catchup` marks the large answer to `join`, which counts against no limit of its pipe.
    Send {
        to: Vec<PipeId>,
        frame: Arc<[u8]>,
        catchup: bool,
    },
    Drop {
        pipe: PipeId,
    },
    /// Writes this change to the journal before anyone hears of it.
    Append(Vec<u8>),
    KeepAsset(Hash),
    Unsaved,
    /// The new file reads well, so it is the file on disk now.
    FileTaken,
    Write {
        rev: Rev,
        file: Vec<u8>,
    },
    /// A change must leave no trace: save at once, then start a new journal.
    Seal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    pub epoch: u64,
    pub first_rev: Rev,
    pub seqs: HashMap<String, Seq>,
    /// Revs that were saved. The file on disk holds the last of them.
    pub saved: Vec<Rev>,
}

struct Seat {
    viewer: Viewer,
    client: Option<String>,
    place: Value,
}

/// Takes an input and returns effects, and holds no network, disk, clock or thread.
pub struct Document<F: Format> {
    model: F::Model,
    assets: Assets,
    original: Vec<u8>,
    epoch: u64,
    first_rev: Rev,
    rev: Rev,
    log: Vec<Ordered<F::Change>>,
    seqs: HashMap<String, Seq>,
    seats: BTreeMap<PipeId, Seat>,
    saved_rev: Rev,
    saved_states: Vec<Rev>,
    waits: bool,
}

impl<F: Format> Document<F> {
    pub fn open(file: Vec<u8>, epoch: u64, assets: Assets) -> Result<Self, Unreadable> {
        F::opens(&file)?;
        Self::start(file, epoch, assets)
    }

    fn start(file: Vec<u8>, epoch: u64, mut assets: Assets) -> Result<Self, Unreadable> {
        let model = F::read(&file, &mut assets)?;
        Ok(Self {
            model,
            assets,
            original: file,
            epoch,
            first_rev: 0,
            rev: 0,
            log: Vec::new(),
            seqs: HashMap::new(),
            seats: BTreeMap::new(),
            saved_rev: 0,
            saved_states: vec![0],
            waits: false,
        })
    }

    pub fn restore(
        original: Vec<u8>,
        restored: Restored,
        records: &[Vec<u8>],
        assets: Assets,
    ) -> Result<Self, Unreadable> {
        let mut document = Self::start(original, restored.epoch, assets)?;
        document.first_rev = restored.first_rev;
        document.rev = restored.first_rev;
        document.saved_rev = restored.saved.last().copied().unwrap_or(restored.first_rev);
        document.saved_states = restored.saved;
        document.saved_states.insert(0, restored.first_rev);
        document.seqs = restored.seqs;
        for record in records {
            let changed: Changed = serde_json::from_slice(record)
                .map_err(|error| Unreadable(format!("the journal is broken: {error}")))?;
            document.replay(changed)?;
        }
        Ok(document)
    }

    fn replay(&mut self, changed: Changed) -> Result<(), Unreadable> {
        let change: F::Change = serde_json::from_value(changed.body)
            .map_err(|error| Unreadable(format!("the journal is broken: {error}")))?;
        let change = F::apply(&mut self.model, change, &[], &mut self.assets)
            .map_err(|error| Unreadable(format!("the journal is broken: {error}")))?;
        self.rev = changed.rev;
        if changed.seq > 0 {
            self.seqs.insert(changed.client.clone(), changed.seq);
        }
        self.log.push(Ordered {
            rev: changed.rev,
            author: changed.author,
            client: changed.client,
            seq: changed.seq,
            change,
        });
        Ok(())
    }

    pub fn model(&self) -> &F::Model {
        &self.model
    }

    pub fn rev(&self) -> Rev {
        self.rev
    }

    pub fn saved_rev(&self) -> Rev {
        self.saved_rev
    }

    pub fn seqs(&self) -> &HashMap<String, Seq> {
        &self.seqs
    }

    pub fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub fn asset(&self, hash: &Hash) -> Option<&Arc<[u8]>> {
        self.assets.get(hash)
    }

    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        match input {
            Input::Connected { pipe, viewer } => {
                self.seats.insert(
                    pipe,
                    Seat {
                        viewer,
                        client: None,
                        place: Value::Null,
                    },
                );
                Vec::new()
            }
            Input::Message { pipe, frame } => self.message(pipe, &frame),
            Input::Left { pipe } => self.left(pipe),
            Input::FileChanged { file, writer } => self.file_changed(&file, &writer),
            Input::Save { file } => self.save(&file),
            Input::Saved { rev } => self.saved(rev),
            Input::SaveWaits => self.save_waits(),
            Input::Rebased { file, epoch } => self.rebased(file, epoch),
            Input::Closing { reason } => {
                let frame = contract::encode_message(&ToClient::Closed { reason });
                vec![Effect::Send {
                    to: self.seats.keys().copied().collect(),
                    frame: frame.into(),
                    catchup: false,
                }]
            }
        }
    }

    fn message(&mut self, pipe: PipeId, frame: &[u8]) -> Vec<Effect> {
        let Ok(decoded) = contract::decode::<ToDocument>(frame) else {
            return vec![Effect::Drop { pipe }];
        };
        let message = match decoded {
            Frame::Asset { hash, bytes } => return self.asset_frame(pipe, hash, &bytes),
            Frame::Message(message) => message,
        };
        let joined = self
            .seats
            .get(&pipe)
            .is_some_and(|seat| seat.client.is_some());
        match message {
            ToDocument::Join {
                contract,
                client,
                epoch,
                rev,
            } if !joined => self.join(pipe, contract, client, epoch, rev),
            ToDocument::Change { seq, base, body } if joined => self.change(pipe, seq, base, body),
            ToDocument::Place { rev, body } if joined => self.place(pipe, rev, body),
            ToDocument::Query { id, body } if joined => self.query(pipe, id, body),
            ToDocument::People { id } if joined => self.people(pipe, id),
            _ => vec![Effect::Drop { pipe }],
        }
    }

    fn asset_frame(&mut self, pipe: PipeId, hash: Hash, bytes: &[u8]) -> Vec<Effect> {
        if bytes.is_empty() {
            return self.assets.get(&hash).map_or_else(Vec::new, |asset| {
                vec![Effect::Send {
                    to: vec![pipe],
                    frame: contract::encode_asset(&hash, asset).into(),
                    catchup: true,
                }]
            });
        }
        if self.assets.bytes() + bytes.len() > MOST_ASSET_BYTES || !self.assets.take(hash, bytes) {
            return vec![Effect::Drop { pipe }];
        }
        vec![Effect::KeepAsset(hash)]
    }

    fn join(
        &mut self,
        pipe: PipeId,
        contract_number: u32,
        client: String,
        epoch: u64,
        rev: Rev,
    ) -> Vec<Effect> {
        if contract_number != contract::CONTRACT {
            let closed = ToClient::Closed {
                reason: ClosedReason::Contract,
            };
            return vec![send_to(vec![pipe], &closed), Effect::Drop { pipe }];
        }
        let mut effects: Vec<Effect> = self
            .seats
            .iter()
            .filter(|(_, seat)| seat.client.as_ref() == Some(&client))
            .map(|(pipe, _)| Effect::Drop { pipe: *pipe })
            .collect();
        for effect in &effects {
            if let Effect::Drop { pipe } = effect {
                self.seats.remove(pipe);
            }
        }
        let catchup = self.catchup(epoch, rev);
        let peers = self.peers();
        let Some(seat) = self.seats.get_mut(&pipe) else {
            return effects;
        };
        seat.client = Some(client.clone());
        let joined = ToClient::Joined {
            id: client.clone(),
            name: seat.viewer.name.clone(),
            can_edit: seat.viewer.can_edit,
            epoch: self.epoch,
            rev: self.rev,
            seq: self.seqs.get(&client).copied().unwrap_or(0),
            peers,
            catchup,
        };
        let peer = ToClient::Peer(Peer {
            id: client,
            person: seat.viewer.id.clone(),
            name: seat.viewer.name.clone(),
            place: Value::Null,
        });
        effects.push(Effect::Send {
            to: vec![pipe],
            frame: contract::encode_message(&joined).into(),
            catchup: true,
        });
        effects.push(send_to(self.others(pipe), &peer));
        effects
    }

    /// A new view knows epoch 0.
    fn catchup(&self, epoch: u64, rev: Rev) -> Catchup {
        let missed = (epoch != 0 && epoch == self.epoch)
            .then(|| self.log_since(rev))
            .flatten();
        missed.map_or_else(
            || Catchup::Snapshot {
                snapshot: F::snapshot(&self.model).0,
            },
            |missed| Catchup::Missed {
                changes: missed.iter().map(changed_of).collect(),
            },
        )
    }

    fn log_since(&self, rev: Rev) -> Option<&[Ordered<F::Change>]> {
        let skip = usize::try_from(rev.checked_sub(self.first_rev)?).ok()?;
        self.log.get(skip..)
    }

    fn peers(&self) -> Vec<Peer> {
        self.seats
            .values()
            .filter_map(|seat| {
                Some(Peer {
                    id: seat.client.clone()?,
                    person: seat.viewer.id.clone(),
                    name: seat.viewer.name.clone(),
                    place: seat.place.clone(),
                })
            })
            .collect()
    }

    fn others(&self, pipe: PipeId) -> Vec<PipeId> {
        self.joined_pipes().filter(|other| *other != pipe).collect()
    }

    fn joined_pipes(&self) -> impl Iterator<Item = PipeId> {
        self.seats
            .iter()
            .filter(|(_, seat)| seat.client.is_some())
            .map(|(pipe, _)| *pipe)
    }

    fn change(&mut self, pipe: PipeId, seq: Seq, base: Rev, body: Value) -> Vec<Effect> {
        let Some(seat) = self.seats.get(&pipe) else {
            return Vec::new();
        };
        let (Some(client), author) = (seat.client.clone(), seat.viewer.name.clone()) else {
            return Vec::new();
        };
        if !seat.viewer.can_edit {
            let detail = "This person can only read the file.".to_owned();
            return vec![refuse(pipe, seq, RefusedReason::Invalid, Some(detail))];
        }
        if seq <= self.seqs.get(&client).copied().unwrap_or(0) {
            return Vec::new();
        }
        let change = match serde_json::from_value::<F::Change>(body) {
            Ok(change) => change,
            Err(error) => {
                let detail = format!("The change is not valid: {error}.");
                return vec![refuse(pipe, seq, RefusedReason::Invalid, Some(detail))];
            }
        };
        if base > self.rev {
            let detail = format!(
                "The file opened again since rev {base} and is at rev {} now. Read again.",
                self.rev
            );
            return vec![refuse(pipe, seq, RefusedReason::Invalid, Some(detail))];
        }
        let Some(since) = self.log_since(base) else {
            let detail = format!("The changes since rev {base} are gone. Read again.");
            return vec![refuse(pipe, seq, RefusedReason::Behind, Some(detail))];
        };
        let since = since.to_vec();
        match F::apply(&mut self.model, change, &since, &mut self.assets) {
            Ok(change) => {
                self.seqs.insert(client.clone(), seq);
                self.ordered(change, author, client, seq)
            }
            Err(Refused::Behind(detail)) => {
                vec![refuse(pipe, seq, RefusedReason::Behind, Some(detail))]
            }
            Err(Refused::Invalid(detail)) => {
                vec![refuse(pipe, seq, RefusedReason::Invalid, Some(detail))]
            }
        }
    }

    fn ordered(
        &mut self,
        change: F::Change,
        author: String,
        client: String,
        seq: Seq,
    ) -> Vec<Effect> {
        self.rev += 1;
        let seals = F::seals(&change);
        for seat in self.seats.values_mut() {
            F::map_place(&mut seat.place, &change);
        }
        let ordered = Ordered {
            rev: self.rev,
            author,
            client,
            seq,
            change,
        };
        let changed = changed_of(&ordered);
        self.log.push(ordered);
        let record = serde_json::to_vec(&changed).unwrap_or_default();
        let mut effects = vec![
            Effect::Append(record),
            send_to(self.joined_pipes().collect(), &ToClient::Changed(changed)),
            Effect::Unsaved,
        ];
        if seals {
            effects.push(Effect::Seal);
        }
        effects
    }

    fn place(&mut self, pipe: PipeId, rev: Rev, mut body: Value) -> Vec<Effect> {
        let Some(since) = self.log_since(rev) else {
            return Vec::new();
        };
        for ordered in since {
            F::map_place(&mut body, &ordered.change);
        }
        let others = self.others(pipe);
        let Some(seat) = self.seats.get_mut(&pipe) else {
            return Vec::new();
        };
        seat.place.clone_from(&body);
        let Some(id) = seat.client.clone() else {
            return Vec::new();
        };
        let peer = ToClient::Peer(Peer {
            id,
            person: seat.viewer.id.clone(),
            name: seat.viewer.name.clone(),
            place: body,
        });
        vec![send_to(others, &peer)]
    }

    fn people(&self, pipe: PipeId, id: u64) -> Vec<Effect> {
        let people = self
            .seats
            .iter()
            .filter(|(other, seat)| **other != pipe && seat.client.is_some())
            .map(|(_, seat)| {
                json!({
                    "name": seat.viewer.name,
                    "place": F::describe_place(&self.model, &seat.place),
                })
            })
            .collect();
        let answer = ToClient::Answer {
            id,
            rev: self.rev,
            body: Value::Array(people),
        };
        vec![send_to(vec![pipe], &answer)]
    }

    fn query(&self, pipe: PipeId, id: u64, body: Value) -> Vec<Effect> {
        let Ok(query) = serde_json::from_value::<F::Query>(body) else {
            return vec![Effect::Drop { pipe }];
        };
        let answer = ToClient::Answer {
            id,
            rev: self.rev,
            body: F::query(&self.model, query).0,
        };
        vec![send_to(vec![pipe], &answer)]
    }

    fn left(&mut self, pipe: PipeId) -> Vec<Effect> {
        let Some(id) = self.seats.remove(&pipe).and_then(|seat| seat.client) else {
            return Vec::new();
        };
        vec![send_to(
            self.joined_pipes().collect(),
            &ToClient::Left { id },
        )]
    }

    /// Merges a write of another program, as Git merges: the write's own changes are what lies
    /// between the saved state that it started from and the new file.
    fn file_changed(&mut self, file: &[u8], writer: &str) -> Vec<Effect> {
        let mut assets = self.assets.clone();
        let Ok(new) = F::read(file, &mut assets) else {
            return Vec::new();
        };
        self.assets = assets;
        let Some(changes) = self.nearest_changes(&new) else {
            return Vec::new();
        };
        let mut effects = vec![Effect::FileTaken];
        for change in changes {
            if let Ok(change) = F::apply(&mut self.model, change, &[], &mut self.assets) {
                effects.extend(self.ordered(
                    change,
                    writer.to_owned(),
                    format!("file:{writer}"),
                    0,
                ));
            }
        }
        effects
    }

    fn nearest_changes(&self, new: &F::Model) -> Option<Vec<F::Change>> {
        let mut assets = self.assets.clone();
        let mut state = F::read(&self.original, &mut assets).ok()?;
        let mut applied = self.first_rev;
        let mut nearest: Option<(usize, Vec<F::Change>)> = None;
        let mut log = self.log.iter();
        for saved in self.saved_states.clone() {
            while applied < saved {
                let ordered = log.next()?;
                F::apply(&mut state, ordered.change.clone(), &[], &mut assets).ok()?;
                applied = ordered.rev;
            }
            let changes = F::compare(&state, new);
            let distance = F::distance(&changes);
            if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
                nearest = Some((distance, changes));
            }
        }
        nearest.map(|(_, changes)| changes)
    }

    fn save(&mut self, file: &[u8]) -> Vec<Effect> {
        if self.rev == self.saved_rev {
            return Vec::new();
        }
        match F::write(&self.model, file, &self.assets) {
            Ok(file) => vec![Effect::Write {
                rev: self.rev,
                file,
            }],
            Err(_) => self.save_waits(),
        }
    }

    fn saved(&mut self, rev: Rev) -> Vec<Effect> {
        self.saved_rev = rev;
        self.waits = false;
        if self.saved_states.last() != Some(&rev) {
            self.saved_states.push(rev);
        }
        if self.saved_states.len() > MOST_SAVED_STATES {
            self.saved_states.remove(1);
        }
        let saved = ToClient::Saved { rev, waits: false };
        vec![send_to(self.joined_pipes().collect(), &saved)]
    }

    fn save_waits(&mut self) -> Vec<Effect> {
        if self.waits {
            return Vec::new();
        }
        self.waits = true;
        let saved = ToClient::Saved {
            rev: self.saved_rev,
            waits: true,
        };
        vec![send_to(self.joined_pipes().collect(), &saved)]
    }

    fn rebased(&mut self, file: Vec<u8>, epoch: u64) -> Vec<Effect> {
        self.original = file;
        self.first_rev = self.rev;
        self.log.clear();
        self.saved_states = vec![self.rev];
        self.saved_rev = self.rev;
        self.epoch = epoch;
        Vec::new()
    }
}

fn send_to(to: Vec<PipeId>, message: &ToClient) -> Effect {
    Effect::Send {
        to,
        frame: contract::encode_message(message).into(),
        catchup: false,
    }
}

fn refuse(pipe: PipeId, seq: Seq, reason: RefusedReason, detail: Option<String>) -> Effect {
    send_to(
        vec![pipe],
        &ToClient::Refused {
            seq,
            reason,
            detail,
        },
    )
}

fn changed_of<C: serde::Serialize>(ordered: &Ordered<C>) -> Changed {
    Changed {
        rev: ordered.rev,
        author: ordered.author.clone(),
        client: ordered.client.clone(),
        seq: ordered.seq,
        body: serde_json::to_value(&ordered.change).unwrap_or(Value::Null),
    }
}
