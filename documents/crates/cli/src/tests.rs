use std::path::{Path, PathBuf};

use document::{Frame, ToClient};
use documents::{Documents, Viewer};
use serde_json::{Value, json};

use crate::{Agents, ApplyArguments, Command, Failed, ReadArguments, SEAT_KEPT, formats};

struct Rig {
    folder: tempfile::TempDir,
    documents: Documents,
    agents: Agents,
    agent: Viewer,
}

fn rig() -> Rig {
    let folder = tempfile::tempdir().unwrap();
    let documents = Documents::new(folder.path().join("state"), formats());
    Rig {
        agents: Agents::new(documents.clone()),
        documents,
        folder,
        agent: Viewer {
            id: "agent".to_owned(),
            name: "Agent".to_owned(),
            can_edit: true,
        },
    }
}

impl Rig {
    fn path(&self, name: &str) -> PathBuf {
        self.folder.path().join(name)
    }

    async fn run(&self, command: Command) -> Result<Value, Failed> {
        let text = self.agents.run(&self.agent, command).await?;
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    async fn read(&self, file: &Path) -> Value {
        self.run(Command::Read(ReadArguments {
            file: file.to_owned(),
            place: None,
        }))
        .await
        .unwrap()
    }

    async fn apply(&self, file: &Path, base: u64, ops: Value) -> Result<Value, Failed> {
        self.run(Command::Apply(ApplyArguments {
            file: file.to_owned(),
            ops: ops.to_string(),
            base,
            track: false,
        }))
        .await
    }
}

fn rev_of(read: &Value) -> u64 {
    read["rev"].as_u64().unwrap()
}

#[tokio::test]
async fn an_agent_waits_for_its_own_answer_and_not_for_a_change_of_the_same_number() {
    let rig = rig();
    let file = rig.path("notes.txt");
    std::fs::write(&file, "one\ntwo\n").unwrap();
    let base = rev_of(&rig.read(&file).await);
    let other = Viewer {
        id: "other".to_owned(),
        name: "Other".to_owned(),
        can_edit: true,
    };
    rig.agents
        .run(
            &other,
            Command::Apply(ApplyArguments {
                file: file.clone(),
                ops: json!([{ "op": "insertLines", "text": "zero", "after": 0 }]).to_string(),
                base,
                track: false,
            }),
        )
        .await
        .unwrap();
    let late = rig
        .apply(&file, base, json!([{ "op": "deleteLines", "from": 2 }]))
        .await;
    assert!(
        matches!(&late, Err(Failed::Refused(detail)) if detail.contains("Other")),
        "{late:?}"
    );
}

#[tokio::test]
async fn an_agent_that_stops_leaves_the_document_by_itself() {
    let rig = rig();
    let file = rig.path("notes.txt");
    std::fs::write(&file, "one\n").unwrap();
    rig.read(&file).await;
    let mut person = rig
        .documents
        .open(
            &file,
            Viewer {
                id: "ana".to_owned(),
                name: "Ana".to_owned(),
                can_edit: true,
            },
        )
        .await
        .unwrap();
    let join = document::ToDocument::Join {
        contract: document::CONTRACT,
        client: "ana".to_owned(),
        epoch: 0,
        rev: 0,
        fresh: true,
    };
    person
        .send(document::contract::encode_message(&join))
        .await
        .unwrap();
    let mut agent = None;
    tokio::time::pause();
    let left = tokio::time::timeout(SEAT_KEPT * 2, async {
        loop {
            let frame = person.receive().await.unwrap();
            match document::contract::decode(&frame).unwrap() {
                Frame::Message(ToClient::Joined { peers, .. }) => agent = Some(peers[0].id.clone()),
                Frame::Message(ToClient::Left { id }) => return id,
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(left.ok(), agent);
}
