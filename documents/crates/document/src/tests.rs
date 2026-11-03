use std::collections::{BTreeMap, HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Answer, Assets, CONTRACT, Catchup, Changed, Document, Effect, Format, Frame, Guide, Input,
    Ordered, PipeId, Refused, RefusedReason, Restored, Rev, Seq, Snapshot, ToClient, ToDocument,
    Unreadable, Unwritable, Viewer, contract,
};

/// A small format with both kinds of change: cells that a key names, and lines that a position
/// names.
struct Sheet;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Model {
    cells: BTreeMap<String, String>,
    lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Change {
    Set { key: String, value: String },
    Insert { at: usize, line: String },
    Seal,
}

impl Format for Sheet {
    type Model = Model;
    type Change = Change;
    type Query = String;

    fn read(file: &[u8], _assets: &mut Assets) -> Result<Model, Unreadable> {
        serde_json::from_slice(file).map_err(|error| Unreadable(error.to_string()))
    }

    fn snapshot(model: &Model) -> Snapshot {
        Snapshot(json!(model))
    }

    fn apply(
        model: &mut Model,
        change: Change,
        since: &[Ordered<Change>],
        _assets: &mut Assets,
    ) -> Result<Change, Refused> {
        match &change {
            Change::Set { key, value } => {
                model.cells.insert(key.clone(), value.clone());
            }
            Change::Insert { at, line } => {
                let moved = since
                    .iter()
                    .any(|ordered| matches!(ordered.change, Change::Insert { .. }));
                if moved {
                    return Err(Refused::Behind("lines moved".to_owned()));
                }
                if *at > model.lines.len() {
                    return Err(Refused::Invalid("no such line".to_owned()));
                }
                model.lines.insert(*at, line.clone());
            }
            Change::Seal => {}
        }
        Ok(change)
    }

    fn compare(from: &Model, to: &Model) -> Vec<Change> {
        let cells = to
            .cells
            .iter()
            .filter(|(key, value)| from.cells.get(*key) != Some(value))
            .map(|(key, value)| Change::Set {
                key: key.clone(),
                value: value.clone(),
            });
        let lines = to
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !from.lines.contains(line))
            .map(|(at, line)| Change::Insert {
                at: at.min(from.lines.len()),
                line: line.clone(),
            });
        cells.chain(lines).collect()
    }

    fn write(model: &Model, _file: &[u8], _assets: &Assets) -> Result<Vec<u8>, Unwritable> {
        serde_json::to_vec(model).map_err(|error| Unwritable(error.to_string()))
    }

    fn query(model: &Model, query: String) -> Answer {
        Answer(json!(model.cells.get(&query)))
    }

    fn guide() -> Guide {
        Guide {
            format: "sheet".to_owned(),
            ops: Vec::new(),
        }
    }

    fn seals(change: &Change) -> bool {
        matches!(change, Change::Seal)
    }
}

fn file_of(model: &Model) -> Vec<u8> {
    serde_json::to_vec(model).unwrap()
}

fn viewer(name: &str) -> Viewer {
    Viewer {
        id: name.to_owned(),
        name: name.to_owned(),
        can_edit: true,
    }
}

fn opened() -> Document<Sheet> {
    Document::open(file_of(&Model::default()), 1, Assets::default()).unwrap()
}

fn frame(message: &ToDocument) -> Vec<u8> {
    contract::encode_message(message)
}

fn join_frame(client: &str, epoch: u64, rev: Rev) -> Vec<u8> {
    frame(&ToDocument::Join {
        contract: CONTRACT,
        client: client.to_owned(),
        epoch,
        rev,
    })
}

fn change_frame(seq: Seq, base: Rev, change: &Change) -> Vec<u8> {
    frame(&ToDocument::Change {
        seq,
        base,
        body: json!(change),
    })
}

fn set(key: &str, value: &str) -> Change {
    Change::Set {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

fn message_to(document: &mut Document<Sheet>, pipe: PipeId, frame: Vec<u8>) -> Vec<Effect> {
    document.step(Input::Message { pipe, frame })
}

fn connect(document: &mut Document<Sheet>, pipe: PipeId, name: &str) -> Vec<Effect> {
    document.step(Input::Connected {
        pipe,
        viewer: viewer(name),
    });
    message_to(document, pipe, join_frame(name, 0, 0))
}

fn messages_for(pipe: PipeId, effects: &[Effect]) -> Vec<ToClient> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { to, frame, .. } if to.contains(&pipe) => {
                match contract::decode::<ToClient>(frame) {
                    Ok(Frame::Message(message)) => Some(message),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_change_goes_to_the_journal_before_any_view_hears_of_it() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    let effects = message_to(&mut document, 1, change_frame(1, 0, &set("a1", "x")));
    assert!(matches!(effects[0], Effect::Append(_)));
    assert!(matches!(effects[1], Effect::Send { .. }));
    assert_eq!(document.rev(), 1);
}

#[test]
fn a_batch_sent_twice_applies_once() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    let change = change_frame(1, 0, &set("a1", "x"));
    message_to(&mut document, 1, change.clone());
    assert_eq!(message_to(&mut document, 1, change), Vec::new());
    assert_eq!(document.rev(), 1);
}

#[test]
fn every_view_hears_a_change_and_the_sender_too() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    connect(&mut document, 2, "ben");
    let effects = message_to(&mut document, 1, change_frame(1, 0, &set("a1", "x")));
    for pipe in [1, 2] {
        assert!(matches!(
            messages_for(pipe, &effects).as_slice(),
            [ToClient::Changed(Changed { rev: 1, .. })]
        ));
    }
}

#[test]
fn a_change_on_old_positions_is_refused_and_a_key_is_not() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    connect(&mut document, 2, "ben");
    let first = Change::Insert {
        at: 0,
        line: "one".to_owned(),
    };
    message_to(&mut document, 1, change_frame(1, 0, &first));
    let late = Change::Insert {
        at: 0,
        line: "two".to_owned(),
    };
    let effects = message_to(&mut document, 2, change_frame(1, 0, &late));
    assert!(matches!(
        messages_for(2, &effects).as_slice(),
        [ToClient::Refused {
            seq: 1,
            reason: RefusedReason::Behind,
            ..
        }]
    ));
    assert_eq!(document.rev(), 1);
    let again = message_to(&mut document, 2, change_frame(1, 1, &late));
    assert!(matches!(
        messages_for(2, &again).as_slice(),
        [ToClient::Changed(_)]
    ));
    let key = message_to(&mut document, 1, change_frame(2, 0, &set("a1", "x")));
    assert!(matches!(
        messages_for(1, &key).as_slice(),
        [ToClient::Changed(Changed { rev: 3, .. })]
    ));
}

#[test]
fn a_viewer_that_cannot_edit_is_refused() {
    let mut document = opened();
    document.step(Input::Connected {
        pipe: 1,
        viewer: Viewer {
            can_edit: false,
            ..viewer("ana")
        },
    });
    message_to(&mut document, 1, join_frame("ana", 0, 0));
    let effects = message_to(&mut document, 1, change_frame(1, 0, &set("a1", "x")));
    assert!(matches!(
        messages_for(1, &effects).as_slice(),
        [ToClient::Refused {
            reason: RefusedReason::Invalid,
            ..
        }]
    ));
    assert_eq!(document.rev(), 0);
}

#[test]
fn another_contract_number_cannot_join() {
    let mut document = opened();
    document.step(Input::Connected {
        pipe: 1,
        viewer: viewer("ana"),
    });
    let effects = message_to(
        &mut document,
        1,
        frame(&ToDocument::Join {
            contract: CONTRACT + 1,
            client: "ana".to_owned(),
            epoch: 0,
            rev: 0,
        }),
    );
    assert!(matches!(
        messages_for(1, &effects).as_slice(),
        [ToClient::Closed { .. }]
    ));
    assert!(effects.contains(&Effect::Drop { pipe: 1 }));
}

#[test]
fn a_view_that_comes_back_gets_only_what_it_missed() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    message_to(&mut document, 1, change_frame(1, 0, &set("a1", "x")));
    message_to(&mut document, 1, change_frame(2, 1, &set("a2", "y")));
    document.step(Input::Left { pipe: 1 });
    document.step(Input::Connected {
        pipe: 2,
        viewer: viewer("ana"),
    });
    let effects = message_to(&mut document, 2, join_frame("ana", 1, 1));
    match messages_for(2, &effects).as_slice() {
        [
            ToClient::Joined {
                seq: 2,
                rev: 2,
                catchup: Catchup::Missed { changes },
                ..
            },
        ] => assert_eq!(changes.len(), 1),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_view_from_an_earlier_journal_gets_a_snapshot() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    message_to(&mut document, 1, change_frame(1, 0, &set("a1", "x")));
    document.step(Input::Connected {
        pipe: 2,
        viewer: viewer("ben"),
    });
    let effects = message_to(&mut document, 2, join_frame("ben", 7, 1));
    assert!(matches!(
        messages_for(2, &effects).as_slice(),
        [ToClient::Joined {
            catchup: Catchup::Snapshot { .. },
            ..
        }]
    ));
}

#[test]
fn a_new_pipe_of_one_client_replaces_the_old_one() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    document.step(Input::Connected {
        pipe: 2,
        viewer: viewer("ana"),
    });
    let effects = message_to(&mut document, 2, join_frame("ana", 1, 0));
    assert!(effects.contains(&Effect::Drop { pipe: 1 }));
    assert_eq!(document.step(Input::Left { pipe: 1 }), Vec::new());
}

#[test]
fn places_go_to_the_others_and_not_to_the_journal() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    connect(&mut document, 2, "ben");
    let effects = message_to(
        &mut document,
        1,
        frame(&ToDocument::Place {
            rev: 0,
            body: json!({ "cell": "B2" }),
        }),
    );
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        messages_for(2, &effects).as_slice(),
        [ToClient::Peer(peer)] if peer.place == json!({ "cell": "B2" })
    ));
    assert_eq!(messages_for(1, &effects), Vec::new());
    document.step(Input::Left { pipe: 1 });
    document.step(Input::Connected {
        pipe: 3,
        viewer: viewer("cem"),
    });
    let joined = message_to(&mut document, 3, join_frame("cem", 0, 0));
    assert!(matches!(
        messages_for(3, &joined).as_slice(),
        [ToClient::Joined { peers, .. }] if peers.len() == 1
    ));
}

#[test]
fn a_query_sees_the_live_copy_with_its_rev() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    message_to(&mut document, 1, change_frame(1, 0, &set("a1", "x")));
    let effects = message_to(
        &mut document,
        1,
        frame(&ToDocument::Query {
            id: 5,
            body: json!("a1"),
        }),
    );
    assert_eq!(
        messages_for(1, &effects),
        [ToClient::Answer {
            id: 5,
            rev: 1,
            body: json!("x"),
        }]
    );
}

#[test]
fn a_save_with_no_change_writes_nothing() {
    let mut document = opened();
    assert_eq!(document.step(Input::Save { file: Vec::new() }), Vec::new());
}

#[test]
fn a_waiting_save_shows_once_and_ends_when_saved() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    let waits = document.step(Input::SaveWaits);
    assert_eq!(
        messages_for(1, &waits),
        [ToClient::Saved {
            rev: 0,
            waits: true,
        }]
    );
    assert_eq!(document.step(Input::SaveWaits), Vec::new());
    let saved = document.step(Input::Saved { rev: 0 });
    assert_eq!(
        messages_for(1, &saved),
        [ToClient::Saved {
            rev: 0,
            waits: false,
        }]
    );
}

#[test]
fn a_seal_asks_for_a_save_at_once() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    let effects = message_to(&mut document, 1, change_frame(1, 0, &Change::Seal));
    assert!(effects.contains(&Effect::Seal));
}

#[test]
fn a_late_write_keeps_the_edits_made_while_the_program_worked() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    message_to(&mut document, 1, change_frame(1, 0, &set("a1", "first")));
    let saved = document.model().clone();
    document.step(Input::Saved { rev: 1 });
    message_to(&mut document, 1, change_frame(2, 1, &set("a2", "second")));
    message_to(&mut document, 1, change_frame(3, 2, &set("a3", "third")));
    let mut written = saved;
    written.cells.insert("b1".to_owned(), "script".to_owned());
    let effects = document.step(Input::FileChanged {
        file: file_of(&written),
        writer: "script".to_owned(),
    });
    let model = document.model();
    assert_eq!(model.cells["a2"], "second");
    assert_eq!(model.cells["a3"], "third");
    assert_eq!(model.cells["b1"], "script");
    assert!(matches!(
        messages_for(1, &effects).as_slice(),
        [ToClient::Changed(Changed { author, .. })] if author == "script"
    ));
}

#[test]
fn a_half_written_file_is_ignored() {
    let mut document = opened();
    let effects = document.step(Input::FileChanged {
        file: b"{ half".to_vec(),
        writer: "script".to_owned(),
    });
    assert_eq!(effects, Vec::new());
}

struct Packet {
    pipe: PipeId,
    frame: Vec<u8>,
}

/// One view: its confirmed copy, and its edits that wait for an answer.
struct View {
    client: String,
    pipe: Option<PipeId>,
    model: Model,
    epoch: u64,
    rev: Rev,
    next_seq: Seq,
    pending: Option<(Seq, Change)>,
    queued: VecDeque<Change>,
    inbox: VecDeque<Vec<u8>>,
    last_moved: Rev,
}

impl View {
    fn new(client: &str) -> Self {
        Self {
            client: client.to_owned(),
            pipe: None,
            model: Model::default(),
            epoch: 0,
            rev: 0,
            next_seq: 1,
            pending: None,
            queued: VecDeque::new(),
            inbox: VecDeque::new(),
            last_moved: 0,
        }
    }

    fn applied(&mut self, changed: &Changed) {
        let change: Change = serde_json::from_value(changed.body.clone()).unwrap();
        match change {
            Change::Set { key, value } => {
                self.model.cells.insert(key, value);
            }
            Change::Insert { at, line } => {
                self.model.lines.insert(at, line);
                self.last_moved = changed.rev;
            }
            Change::Seal => {}
        }
        self.rev = changed.rev;
    }

    fn take_pending(&mut self, seq: Seq) {
        if self.pending.as_ref().is_some_and(|(mine, _)| *mine <= seq) {
            self.pending = None;
        }
    }

    fn next_frame(&mut self) -> Option<Vec<u8>> {
        if self.pending.is_none() {
            let change = self.queued.pop_front()?;
            self.pending = Some((self.next_seq, change));
            self.next_seq += 1;
            return Some(change_frame(
                self.next_seq - 1,
                self.rev,
                &self.pending.as_ref()?.1,
            ));
        }
        None
    }

    fn resend(&mut self) -> Option<Vec<u8>> {
        let rev = self.rev;
        let lines = self.model.lines.len();
        let (seq, change) = self.pending.as_mut()?;
        if let Change::Insert { at, .. } = change {
            *at = (*at).min(lines);
        }
        Some(change_frame(*seq, rev, change))
    }

    /// Takes one message, and returns what it sends in answer.
    fn receive(&mut self, frame: &[u8]) -> Vec<Vec<u8>> {
        let Ok(Frame::Message(message)) = contract::decode::<ToClient>(frame) else {
            return Vec::new();
        };
        match message {
            ToClient::Joined {
                epoch,
                rev,
                seq,
                catchup,
                ..
            } => {
                self.epoch = epoch;
                match catchup {
                    Catchup::Snapshot { snapshot } => {
                        self.model = serde_json::from_value(snapshot).unwrap();
                        self.rev = rev;
                    }
                    Catchup::Missed { changes } => {
                        for changed in &changes {
                            self.applied(changed);
                        }
                    }
                }
                self.take_pending(seq);
                self.resend()
                    .or_else(|| self.next_frame())
                    .into_iter()
                    .collect()
            }
            ToClient::Changed(changed) => {
                self.applied(&changed);
                if changed.client == self.client {
                    self.take_pending(changed.seq);
                }
                self.next_frame().into_iter().collect()
            }
            ToClient::Refused { reason, .. } => {
                assert_eq!(reason, RefusedReason::Behind);
                self.resend().into_iter().collect()
            }
            _ => Vec::new(),
        }
    }
}

/// A small random source, so each seed gives one exact run.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// A server with its journal, many views, late packets, lost links, restarts and outside
/// writes.
struct World {
    document: Document<Sheet>,
    original: Vec<u8>,
    records: Vec<Vec<u8>>,
    saved: Vec<Rev>,
    disk: Vec<u8>,
    views: Vec<View>,
    links: HashMap<PipeId, usize>,
    to_server: VecDeque<Packet>,
    next_pipe: PipeId,
    answered: Vec<Changed>,
    random: Random,
}

impl World {
    fn new(seed: u64) -> Self {
        let file = file_of(&Model::default());
        Self {
            document: Document::open(file.clone(), 1, Assets::default()).unwrap(),
            original: file.clone(),
            records: Vec::new(),
            saved: Vec::new(),
            disk: file,
            views: ["ana", "ben", "cem"].map(View::new).into(),
            links: HashMap::new(),
            to_server: VecDeque::new(),
            next_pipe: 1,
            answered: Vec::new(),
            random: Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
        }
    }

    fn run_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send { to, frame, .. } => {
                    for pipe in to {
                        if let Some(view) = self.links.get(&pipe) {
                            self.views[*view].inbox.push_back(frame.to_vec());
                        }
                    }
                }
                Effect::Drop { pipe } => self.cut(pipe),
                Effect::Append(record) => {
                    self.answered.push(serde_json::from_slice(&record).unwrap());
                    self.records.push(record);
                }
                Effect::Write { rev, file } => {
                    self.disk = file;
                    self.saved.push(rev);
                    let effects = self.document.step(Input::Saved { rev });
                    self.run_effects(effects);
                }
                Effect::KeepAsset(_) | Effect::Unsaved | Effect::FileTaken | Effect::Seal => {}
            }
        }
    }

    fn cut(&mut self, pipe: PipeId) {
        if let Some(view) = self.links.remove(&pipe) {
            self.views[view].pipe = None;
            self.views[view].inbox.clear();
        }
        self.to_server.retain(|packet| packet.pipe != pipe);
    }

    fn connect(&mut self, view: usize) {
        let pipe = self.next_pipe;
        self.next_pipe += 1;
        self.links.insert(pipe, view);
        self.views[view].pipe = Some(pipe);
        let client = self.views[view].client.clone();
        let effects = self.document.step(Input::Connected {
            pipe,
            viewer: viewer(&client),
        });
        self.run_effects(effects);
        let join = join_frame(&client, self.views[view].epoch, self.views[view].rev);
        self.to_server.push_back(Packet { pipe, frame: join });
    }

    fn lose_link(&mut self, view: usize) {
        if let Some(pipe) = self.views[view].pipe {
            self.cut(pipe);
            let effects = self.document.step(Input::Left { pipe });
            self.run_effects(effects);
        }
    }

    fn deliver_to_server(&mut self) {
        if let Some(packet) = self.to_server.pop_front() {
            let effects = self.document.step(Input::Message {
                pipe: packet.pipe,
                frame: packet.frame,
            });
            self.run_effects(effects);
        }
    }

    fn deliver_to_view(&mut self, view: usize) {
        let Some(frame) = self.views[view].inbox.pop_front() else {
            return;
        };
        let Some(pipe) = self.views[view].pipe else {
            return;
        };
        for frame in self.views[view].receive(&frame) {
            self.to_server.push_back(Packet { pipe, frame });
        }
    }

    fn edit(&mut self, view: usize) {
        let key = self.random.below(5);
        let value = self.random.next();
        let change = if self.random.chance(30) {
            Change::Insert {
                at: 0,
                line: format!("{value}"),
            }
        } else {
            set(&format!("k{key}"), &format!("{value}"))
        };
        let target = &mut self.views[view];
        target.queued.push_back(change);
        if target.pipe.is_some()
            && let Some(frame) = target.next_frame()
            && let Some(pipe) = target.pipe
        {
            self.to_server.push_back(Packet { pipe, frame });
        }
    }

    fn save(&mut self) {
        let effects = self.document.step(Input::Save {
            file: self.disk.clone(),
        });
        self.run_effects(effects);
    }

    fn restart(&mut self) {
        for view in 0..self.views.len() {
            if let Some(pipe) = self.views[view].pipe {
                self.cut(pipe);
            }
        }
        self.to_server.clear();
        let seqs = self
            .answered
            .iter()
            .filter(|changed| changed.seq > 0)
            .map(|changed| (changed.client.clone(), changed.seq))
            .collect();
        self.document = Document::restore(
            self.original.clone(),
            Restored {
                epoch: 1,
                first_rev: 0,
                seqs,
                saved: self.saved.clone(),
            },
            &self.records,
            Assets::default(),
        )
        .unwrap();
    }

    fn outside_write(&mut self) {
        let mut model: Model = serde_json::from_slice(&self.disk).unwrap();
        model
            .cells
            .insert(format!("out{}", self.random.below(3)), "outside".to_owned());
        self.disk = file_of(&model);
        let effects = self.document.step(Input::FileChanged {
            file: self.disk.clone(),
            writer: "script".to_owned(),
        });
        self.run_effects(effects);
    }

    fn settle(&mut self) {
        for view in 0..self.views.len() {
            if self.views[view].pipe.is_none() {
                self.connect(view);
            }
        }
        for _ in 0..100_000 {
            let quiet = self.to_server.is_empty()
                && self.views.iter().all(|view| {
                    view.inbox.is_empty() && view.pending.is_none() && view.queued.is_empty()
                });
            if quiet {
                return;
            }
            self.deliver_to_server();
            for view in 0..self.views.len() {
                self.deliver_to_view(view);
            }
        }
        panic!("the views did not settle");
    }

    fn step(&mut self) {
        let view = usize::try_from(self.random.below(3)).unwrap();
        match self.random.below(100) {
            0..=29 => self.edit(view),
            30..=54 => self.deliver_to_server(),
            55..=84 => self.deliver_to_view(view),
            85..=89 => self.lose_link(view),
            90..=94 => {
                if self.views[view].pipe.is_none() {
                    self.connect(view);
                }
            }
            95..=96 => self.save(),
            97 => self.restart(),
            _ => self.outside_write(),
        }
    }
}

#[test]
fn every_view_ends_equal_and_no_answered_change_is_lost() {
    for seed in 1..=200 {
        let mut world = World::new(seed);
        for view in 0..3 {
            world.connect(view);
        }
        for _ in 0..400 {
            world.step();
        }
        world.settle();
        let server = world.document.model().clone();
        for view in &world.views {
            assert_eq!(view.model, server, "seed {seed}, view {}", view.client);
            assert_eq!(view.rev, world.document.rev(), "seed {seed}");
        }
        let mut replayed = Model::default();
        for changed in &world.answered {
            let change = serde_json::from_value(changed.body.clone()).unwrap();
            Sheet::apply(&mut replayed, change, &[], &mut Assets::default()).unwrap();
        }
        assert_eq!(replayed, server, "seed {seed}");
        let revs: Vec<Rev> = world.answered.iter().map(|changed| changed.rev).collect();
        assert_eq!(
            revs,
            (1..=world.document.rev()).collect::<Vec<_>>(),
            "seed {seed}"
        );
    }
}

#[test]
fn a_restart_keeps_seqs_so_a_batch_sent_again_applies_once() {
    let mut world = World::new(9);
    world.connect(0);
    world.deliver_to_server();
    world.deliver_to_view(0);
    world.edit(0);
    world.deliver_to_server();
    let rev = world.document.rev();
    let seq = world.views[0].pending.as_ref().unwrap().0;
    world.restart();
    assert_eq!(world.document.rev(), rev);
    world.connect(0);
    world.settle();
    assert_eq!(world.document.rev(), rev);
    assert_eq!(world.views[0].next_seq, seq + 1);
}

#[test]
fn json_values_that_are_not_changes_are_refused() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    let effects = message_to(
        &mut document,
        1,
        frame(&ToDocument::Change {
            seq: 1,
            base: 0,
            body: Value::Bool(true),
        }),
    );
    assert!(matches!(
        messages_for(1, &effects).as_slice(),
        [ToClient::Refused {
            reason: RefusedReason::Invalid,
            detail: Some(detail),
            ..
        }] if detail.starts_with("The change is not valid")
    ));
}

#[test]
fn a_base_from_before_the_file_opened_again_says_so() {
    let mut document = opened();
    connect(&mut document, 1, "ana");
    let effects = message_to(&mut document, 1, change_frame(1, 4, &set("a1", "x")));
    assert!(matches!(
        messages_for(1, &effects).as_slice(),
        [ToClient::Refused {
            reason: RefusedReason::Invalid,
            detail: Some(detail),
            ..
        }] if detail.contains("rev 0 now")
    ));
}
