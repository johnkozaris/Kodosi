use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use document::{
    Answer, Assets, CONTRACT, ClosedReason, Format, Frame, Guide, Ordered, Refused, Snapshot,
    ToClient, ToDocument, Unreadable, Unwritable, Viewer, contract,
};
use documents::{Documents, Formats, OpenError, Pipe, format};
use serde_json::{Value, json};

struct Notes;

type Model = BTreeMap<String, String>;

impl Format for Notes {
    type Model = Model;
    type Change = (String, String);
    type Query = Value;

    fn read(file: &[u8], _assets: &mut Assets) -> Result<Model, Unreadable> {
        serde_json::from_slice(file).map_err(|error| Unreadable(error.to_string()))
    }

    fn snapshot(model: &Model) -> Snapshot {
        Snapshot(json!(model))
    }

    fn apply(
        model: &mut Model,
        change: (String, String),
        _since: &[Ordered<(String, String)>],
        _assets: &mut Assets,
    ) -> Result<(String, String), Refused> {
        model.insert(change.0.clone(), change.1.clone());
        Ok(change)
    }

    fn compare(from: &Model, to: &Model) -> Vec<(String, String)> {
        to.iter()
            .filter(|(key, value)| from.get(*key) != Some(value))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn write(model: &Model, _file: &[u8], _assets: &Assets) -> Result<Vec<u8>, Unwritable> {
        serde_json::to_vec(model).map_err(|error| Unwritable(error.to_string()))
    }

    fn query(_model: &Model, query: Value) -> Answer {
        Answer(query)
    }

    fn guide() -> Guide {
        Guide {
            format: "notes".to_owned(),
            ops: Vec::new(),
        }
    }
}

struct Rig {
    _folder: tempfile::TempDir,
    state: PathBuf,
    file: PathBuf,
    documents: Documents,
}

fn formats() -> Formats {
    Formats::default().with("notes", format::<Notes>())
}

fn rig() -> Rig {
    let folder = tempfile::tempdir().unwrap();
    let state = folder.path().join("state");
    let file = folder.path().join("a.notes");
    std::fs::write(&file, "{}").unwrap();
    let documents = Documents::new(&state, formats());
    Rig {
        _folder: folder,
        state,
        file,
        documents,
    }
}

fn viewer(name: &str) -> Viewer {
    Viewer {
        id: name.to_owned(),
        name: name.to_owned(),
        can_edit: true,
    }
}

async fn message(pipe: &mut Pipe) -> ToClient {
    let frame = tokio::time::timeout(Duration::from_secs(10), pipe.receive())
        .await
        .expect("a message in time")
        .expect("an open pipe");
    match contract::decode(&frame).unwrap() {
        Frame::Message(message) => message,
        Frame::Asset { .. } => panic!("an asset"),
    }
}

async fn joined(documents: &Documents, path: &Path, name: &str) -> Pipe {
    let mut pipe = documents.open(path, viewer(name)).await.unwrap();
    let join = ToDocument::Join {
        contract: CONTRACT,
        client: name.to_owned(),
        epoch: 0,
        rev: 0,
    };
    pipe.send(contract::encode_message(&join)).await.unwrap();
    assert!(matches!(message(&mut pipe).await, ToClient::Joined { .. }));
    pipe
}

async fn change(pipe: &Pipe, seq: u64, base: u64, key: &str, value: &str) {
    let change = ToDocument::Change {
        seq,
        base,
        body: json!([key, value]),
    };
    pipe.send(contract::encode_message(&change)).await.unwrap();
}

/// A megabyte of text that gzip cannot shrink much.
fn noise(seed: u64) -> String {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(b'a' + u8::try_from(state % 26).unwrap())
        })
        .collect()
}

fn file_value(path: &Path, key: &str) -> Option<String> {
    let file: Model = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    file.get(key).cloned()
}

#[tokio::test]
async fn two_viewers_share_one_order_and_the_file_gets_it() {
    let rig = rig();
    let mut ana = joined(&rig.documents, &rig.file, "ana").await;
    let mut ben = joined(&rig.documents, &rig.file, "ben").await;
    assert!(matches!(message(&mut ana).await, ToClient::Peer(_)));
    change(&ana, 1, 0, "a1", "from ana").await;
    assert!(matches!(message(&mut ben).await, ToClient::Changed(_)));
    assert!(matches!(message(&mut ana).await, ToClient::Changed(_)));
    rig.documents.flush().await;
    assert_eq!(file_value(&rig.file, "a1").as_deref(), Some("from ana"));
    assert!(matches!(
        message(&mut ana).await,
        ToClient::Saved {
            rev: 1,
            waits: false
        }
    ));
}

#[tokio::test]
async fn a_save_comes_by_itself_after_the_last_change() {
    let rig = rig();
    let mut ana = joined(&rig.documents, &rig.file, "ana").await;
    change(&ana, 1, 0, "a1", "x").await;
    assert!(matches!(message(&mut ana).await, ToClient::Changed(_)));
    assert!(matches!(message(&mut ana).await, ToClient::Saved { .. }));
    assert_eq!(file_value(&rig.file, "a1").as_deref(), Some("x"));
}

#[tokio::test]
async fn a_clean_close_saves_and_leaves_no_journal() {
    let rig = rig();
    let mut ana = joined(&rig.documents, &rig.file, "ana").await;
    change(&ana, 1, 0, "a1", "x").await;
    assert!(matches!(message(&mut ana).await, ToClient::Changed(_)));
    rig.documents.close().await;
    assert_eq!(file_value(&rig.file, "a1").as_deref(), Some("x"));
    let left = std::fs::read_dir(&rig.state).unwrap().count();
    assert_eq!(left, 0);
}

#[test]
fn a_restart_keeps_each_change_that_got_an_answer() {
    let folder = tempfile::tempdir().unwrap();
    let state = folder.path().join("state");
    let file = folder.path().join("a.notes");
    std::fs::write(&file, "{}").unwrap();
    let crashed = tokio::runtime::Runtime::new().unwrap();
    crashed.block_on(async {
        let documents = Documents::new(&state, formats());
        let mut ana = joined(&documents, &file, "ana").await;
        change(&ana, 1, 0, "a1", "kept").await;
        assert!(matches!(message(&mut ana).await, ToClient::Changed(_)));
        std::mem::forget(ana);
        std::mem::forget(documents);
    });
    crashed.shutdown_background();
    assert_eq!(file_value(&file, "a1"), None);

    let restarted = tokio::runtime::Runtime::new().unwrap();
    restarted.block_on(async {
        let documents = Documents::new(&state, formats());
        let mut ben = documents.open(&file, viewer("ben")).await.unwrap();
        let join = ToDocument::Join {
            contract: CONTRACT,
            client: "ben".to_owned(),
            epoch: 0,
            rev: 0,
        };
        ben.send(contract::encode_message(&join)).await.unwrap();
        assert!(matches!(
            message(&mut ben).await,
            ToClient::Joined { rev: 1, .. }
        ));
        documents.close().await;
        assert_eq!(file_value(&file, "a1").as_deref(), Some("kept"));
    });
}

#[tokio::test]
async fn a_write_of_another_program_merges_in_its_name() {
    let rig = rig();
    let mut ana = joined(&rig.documents, &rig.file, "ana").await;
    change(&ana, 1, 0, "a1", "from ana").await;
    assert!(matches!(message(&mut ana).await, ToClient::Changed(_)));
    rig.documents.writer(&rig.file, "script").await.unwrap();
    std::fs::write(&rig.file, r#"{"b1":"from script"}"#).unwrap();
    match message(&mut ana).await {
        ToClient::Changed(changed) => assert_eq!(changed.author, "script"),
        other => panic!("{other:?}"),
    }
    rig.documents.close().await;
    assert_eq!(file_value(&rig.file, "a1").as_deref(), Some("from ana"));
    assert_eq!(file_value(&rig.file, "b1").as_deref(), Some("from script"));
}

#[tokio::test]
async fn a_removed_file_closes_the_document_and_says_so() {
    let rig = rig();
    let mut ana = joined(&rig.documents, &rig.file, "ana").await;
    std::fs::remove_file(&rig.file).unwrap();
    assert!(matches!(
        message(&mut ana).await,
        ToClient::Closed {
            reason: ClosedReason::Removed
        }
    ));
}

#[tokio::test]
async fn a_view_that_reads_nothing_loses_its_pipe_and_others_go_on() {
    let rig = rig();
    let slow = joined(&rig.documents, &rig.file, "slow").await;
    let mut ana = joined(&rig.documents, &rig.file, "ana").await;
    let mut unread = slow;
    let mut left = false;
    for seq in 1..=20 {
        change(&ana, seq, seq - 1, "a1", &noise(seq)).await;
        loop {
            match message(&mut ana).await {
                ToClient::Changed(_) => break,
                ToClient::Left { .. } => left = true,
                _ => {}
            }
        }
    }
    assert!(left);
    let mut frames = 0;
    while unread.receive().await.is_some() {
        frames += 1;
    }
    assert!(frames < 20, "{frames} frames waited");
}

#[tokio::test]
async fn only_eight_documents_open_at_once() {
    let rig = rig();
    let folder = rig.file.parent().unwrap().to_owned();
    let mut pipes = Vec::new();
    for index in 0..8 {
        let file = folder.join(format!("{index}.notes"));
        std::fs::write(&file, "{}").unwrap();
        pipes.push(rig.documents.open(&file, viewer("ana")).await.unwrap());
    }
    assert_eq!(pipes.len(), 8);
    let ninth = rig.documents.open(&rig.file, viewer("ana")).await;
    assert!(matches!(ninth, Err(OpenError::TooMany(8))));
}

#[tokio::test]
async fn a_file_that_is_too_large_or_of_an_unknown_kind_does_not_open() {
    let rig = rig();
    let folder = rig.file.parent().unwrap().to_owned();
    let large = folder.join("large.notes");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(33 * 1024 * 1024)
        .unwrap();
    assert!(matches!(
        rig.documents.open(&large, viewer("ana")).await,
        Err(OpenError::TooLarge)
    ));
    let unknown = folder.join("a.exe");
    std::fs::write(&unknown, "").unwrap();
    assert!(matches!(
        rig.documents.open(&unknown, viewer("ana")).await,
        Err(OpenError::UnknownKind)
    ));
}
