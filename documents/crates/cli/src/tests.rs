use std::path::{Path, PathBuf};

use document::{Frame, ToClient};
use documents::{Documents, Viewer};
use serde_json::{Value, json};

use crate::{
    Agents, ApplyArguments, Command, Failed, FileArguments, GuideArguments, NewArguments,
    ReadArguments, SEAT_KEPT, formats,
};

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

    async fn new_file(&self, name: &str, from: Option<(&str, &str)>) -> PathBuf {
        let file = self.path(name);
        let from = from.map(|(source, text)| {
            let path = self.path(source);
            std::fs::write(&path, text).unwrap();
            path
        });
        self.run(Command::New(NewArguments {
            file: file.clone(),
            from,
        }))
        .await
        .unwrap();
        file
    }

    async fn read(&self, file: &Path) -> Value {
        self.run(Command::Read(ReadArguments {
            file: file.to_owned(),
            place: None,
        }))
        .await
        .unwrap()
    }

    async fn apply(
        &self,
        file: &Path,
        base: u64,
        ops: Value,
        track: bool,
    ) -> Result<Value, Failed> {
        self.run(Command::Apply(ApplyArguments {
            file: file.to_owned(),
            ops: ops.to_string(),
            base,
            track,
        }))
        .await
    }
}

fn rev_of(read: &Value) -> u64 {
    read["rev"].as_u64().unwrap()
}

#[tokio::test]
async fn ops_come_only_from_a_file() {
    let rig = rig();
    let file = rig.new_file("a.xlsx", None).await;
    let refused = rig
        .run(Command::Apply(ApplyArguments {
            file,
            ops: rig.folder.path().display().to_string(),
            base: 0,
            track: false,
        }))
        .await;
    assert!(matches!(refused, Err(Failed::Says(text)) if text.ends_with("is not a file.")));
}

#[tokio::test]
async fn an_agent_writes_a_docx_file_through_the_live_document() {
    let rig = rig();
    let file = rig
        .new_file(
            "a.docx",
            Some(("a.md", "# Title\n\nFirst words.\n\n- one\n- two\n")),
        )
        .await;
    let read = rig.read(&file).await;
    let blocks = read["content"]["blocks"].as_array().unwrap();
    assert_eq!(blocks[0]["style"], "Heading1");
    assert_eq!(blocks.len(), 4);
    let answer = rig
        .apply(
            &file,
            rev_of(&read),
            json!([{ "op": "replaceText", "find": "First", "with": "Better" }]),
            false,
        )
        .await
        .unwrap();
    assert_eq!(answer["rev"], 1);
    rig.documents.close().await;
    let file_bytes = std::fs::read(&file).unwrap();
    let rig_again = crate::tests::rig();
    let moved = rig_again.path("a.docx");
    std::fs::write(&moved, file_bytes).unwrap();
    let again = rig_again.read(&moved).await;
    assert_eq!(again["content"]["blocks"][1]["text"], "Better words.");
}

#[tokio::test]
async fn an_agent_edits_a_text_file_by_its_lines_and_it_keeps_its_line_endings() {
    let rig = rig();
    let file = rig.path("config.yaml");
    std::fs::write(&file, "name: one\r\nport: 80\r\n").unwrap();
    let read = |place: &str| {
        rig.run(Command::Read(ReadArguments {
            file: file.clone(),
            place: Some(place.to_owned()),
        }))
    };
    let wrong = read("2-1").await;
    assert!(matches!(wrong, Err(Failed::Says(text)) if text.contains("names no lines")));
    let second = read("2").await.unwrap();
    assert_eq!(
        second["content"],
        json!({ "count": 2, "lines": [{ "line": 2, "text": "port: 80" }] })
    );
    rig.apply(
        &file,
        rev_of(&second),
        json!([
            { "op": "replaceText", "find": "80", "with": "8080" },
            { "op": "insertLines", "text": "host: here", "after": 1 },
        ]),
        false,
    )
    .await
    .unwrap();
    rig.documents.close().await;
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "name: one\r\nhost: here\r\nport: 8080\r\n"
    );
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
        .apply(
            &file,
            base,
            json!([{ "op": "deleteLines", "from": 2 }]),
            false,
        )
        .await;
    assert!(
        matches!(&late, Err(Failed::Refused(detail)) if detail.contains("Other")),
        "{late:?}"
    );
}

#[tokio::test]
async fn a_read_with_a_rev_makes_an_old_apply_fail_with_a_name() {
    let rig = rig();
    let file = rig.new_file("a.xlsx", None).await;
    let read = rig.read(&file).await;
    let base = rev_of(&read);
    rig.apply(
        &file,
        base,
        json!([{ "op": "setCell", "sheet": "Sheet1", "cell": "A1", "value": 1 }]),
        false,
    )
    .await
    .unwrap();
    let other = Viewer {
        id: "other".to_owned(),
        name: "Other".to_owned(),
        can_edit: true,
    };
    let later = rig
        .agents
        .run(
            &other,
            Command::Apply(ApplyArguments {
                file: file.clone(),
                ops: json!([{ "op": "setCell", "sheet": "Sheet1", "cell": "A1", "value": 2 }])
                    .to_string(),
                base,
                track: false,
            }),
        )
        .await;
    match later {
        Err(Failed::Refused(detail)) => assert!(detail.contains("Agent"), "{detail}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_table_from_csv_gives_values_and_formulas() {
    let rig = rig();
    let file = rig
        .new_file("a.xlsx", Some(("a.csv", "name,count\nana,2\nben,3\n")))
        .await;
    let read = rig.read(&file).await;
    rig.apply(
        &file,
        rev_of(&read),
        json!([{ "op": "setCell", "sheet": "Sheet1", "cell": "B4", "formula": "=SUM(B2:B3)" }]),
        false,
    )
    .await
    .unwrap();
    let read = rig.read(&file).await;
    let cells = read["content"]["sheets"][0]["cells"].as_array().unwrap();
    let total = cells.iter().find(|cell| cell["cell"] == "B4").unwrap();
    assert_eq!(total["value"], 5.0);
}

#[tokio::test]
async fn a_deck_from_markdown_has_its_slides() {
    let rig = rig();
    let file = rig
        .new_file(
            "a.pptx",
            Some(("a.md", "# First\n- one\n# Second\n- two\n")),
        )
        .await;
    let read = rig.read(&file).await;
    let slides = read["content"]["slides"].as_array().unwrap();
    assert_eq!(slides.len(), 2);
    assert_eq!(slides[1]["shapes"][0]["text"], "Second");
    assert_eq!(slides[1]["shapes"][1]["text"], "two");
}

#[tokio::test]
async fn the_guide_prints_ops_with_their_fields() {
    let rig = rig();
    let all = rig
        .run(Command::Guide(GuideArguments {
            format: "docx".to_owned(),
            op: None,
        }))
        .await
        .unwrap();
    assert!(all.as_str().unwrap().contains("replaceText"));
    let one = rig
        .run(Command::Guide(GuideArguments {
            format: "xlsx".to_owned(),
            op: Some("setCell".to_owned()),
        }))
        .await
        .unwrap();
    assert!(one.as_str().unwrap().contains("Such as B2"));
    assert!(
        one.as_str()
            .unwrap()
            .contains(r#"{"op": "setCell", "sheet": …, "cell": …}"#)
    );
    assert!(!one.as_str().unwrap().contains("insertRows"));
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

#[tokio::test]
async fn selection_says_who_else_is_here() {
    let rig = rig();
    let file = rig.new_file("a.docx", None).await;
    let read = rig.read(&file).await;
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
    };
    person
        .send(document::contract::encode_message(&join))
        .await
        .unwrap();
    person.receive().await.unwrap();
    let place = document::ToDocument::Place {
        rev: 0,
        body: json!({ "from": 1, "to": 1 }),
    };
    person
        .send(document::contract::encode_message(&place))
        .await
        .unwrap();
    let answer = rig
        .run(Command::Selection(FileArguments { file }))
        .await
        .unwrap();
    let people = answer["people"].as_array().unwrap();
    assert_eq!(people[0]["name"], "Ana");
    assert_eq!(
        people[0]["place"]["block"],
        read["content"]["blocks"][0]["id"]
    );
}
