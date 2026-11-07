//! The live documents of this computer. One server owns each file here, and people and agents
//! are its clients.

use std::path::{Path, PathBuf};

use clap::Parser;
use documents::{Documents, Viewer};
use documents_cli::{Agents, Command, formats};
use tokio::sync::mpsc;

use crate::canvas::{CanvasAnswer, PIPE_QUEUE, PipeEnds};
use crate::text::Text;

const STATE_FOLDER: &str = "documents";

#[derive(Parser)]
#[command(name = "kodosi doc")]
struct DocumentCommand {
    #[command(subcommand)]
    command: Command,
}

/// The documents and the agents' seats in them.
#[derive(Clone)]
pub(crate) struct DocumentHost {
    documents: Documents,
    agents: std::sync::Arc<Agents>,
}

impl DocumentHost {
    pub(crate) fn new(data_root: &Path) -> Self {
        let documents = Documents::new(data_root.join(STATE_FOLDER), formats());
        Self {
            agents: std::sync::Arc::new(Agents::new(documents.clone())),
            documents,
        }
    }

    /// Gives a person a pipe to the live document of a file.
    pub(crate) async fn open(&self, path: &Path, viewer: Viewer) -> Result<PipeEnds, Text> {
        let pipe = self
            .documents
            .open(path, viewer)
            .await
            .map_err(|error| Text::plain(error.to_string()))?;
        Ok(ends_of(pipe))
    }

    /// Runs one `kodosi doc` command of an agent.
    pub(crate) async fn run(
        &self,
        viewer: &Viewer,
        folder: &Path,
        arguments: Vec<String>,
    ) -> CanvasAnswer {
        let words = std::iter::once("doc".to_owned()).chain(arguments);
        let command = match DocumentCommand::try_parse_from(words) {
            Ok(parsed) => parsed.command.rooted(folder),
            Err(error) => {
                return CanvasAnswer::Text {
                    text: error.render().to_string(),
                    code: i32::from(error.use_stderr()) * 2,
                };
            }
        };
        match self.agents.run(viewer, command).await {
            Ok(text) => CanvasAnswer::Text { text, code: 0 },
            Err(error) => CanvasAnswer::Text {
                text: format!("{error}\n"),
                code: 1,
            },
        }
    }

    /// Saves what waits, before the host ends.
    pub(crate) async fn close(&self) {
        self.documents.close().await;
    }
}

/// The ends of a document's pipe, which end when either side lets go.
fn ends_of(mut pipe: documents::Pipe) -> PipeEnds {
    let (sender, mut incoming) = mpsc::channel::<Vec<u8>>(PIPE_QUEUE);
    let (outgoing, receiver) = mpsc::channel(PIPE_QUEUE);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                frame = incoming.recv() => {
                    let Some(frame) = frame else { return };
                    if pipe.send(frame).await.is_err() {
                        return;
                    }
                }
                frame = pipe.receive() => {
                    let Some(frame) = frame else { return };
                    if outgoing.send(frame.to_vec()).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    PipeEnds { sender, receiver }
}

/// The path that a key of the canvas names.
pub(crate) fn path_of(key: &str) -> PathBuf {
    PathBuf::from(key)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn agent() -> Viewer {
        Viewer {
            id: "agent".to_owned(),
            name: "Claude".to_owned(),
            can_edit: true,
        }
    }

    async fn run(host: &DocumentHost, folder: &Path, words: &[&str]) -> (String, i32) {
        let arguments = words.iter().map(|word| (*word).to_owned()).collect();
        match host.run(&agent(), folder, arguments).await {
            CanvasAnswer::Text { text, code } => (text, code),
            other => panic!("text expected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_agent_makes_reads_and_edits_a_file_from_its_folder() {
        let state = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let host = DocumentHost::new(state.path());
        let (_, code) = run(&host, work.path(), &["new", "sums.xlsx"]).await;
        assert_eq!(code, 0);
        let (text, code) = run(&host, work.path(), &["read", "sums.xlsx"]).await;
        assert_eq!(code, 0);
        let read: Value = serde_json::from_str(&text).unwrap();
        let ops = json!([{ "op": "setCell", "sheet": "Sheet1", "cell": "B2", "value": 7 }]);
        let rev = read["rev"].to_string();
        let (text, code) = run(
            &host,
            work.path(),
            &[
                "apply",
                "sums.xlsx",
                "--ops",
                &ops.to_string(),
                "--base",
                &rev,
            ],
        )
        .await;
        assert_eq!(code, 0, "{text}");
        let (text, _) = run(&host, work.path(), &["read", "sums.xlsx", "--where", "B2"]).await;
        assert!(text.contains('7'), "{text}");
        host.close().await;
    }

    #[tokio::test]
    async fn a_wrong_command_says_how_it_is_used() {
        let state = tempfile::tempdir().unwrap();
        let host = DocumentHost::new(state.path());
        let (text, code) = run(&host, state.path(), &["nonsense"]).await;
        assert_eq!(code, 2);
        assert!(text.contains("Usage"), "{text}");
    }

    #[tokio::test]
    async fn a_pipe_carries_frames_both_ways_and_ends_with_either_side() {
        let state = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let host = DocumentHost::new(state.path());
        run(&host, work.path(), &["new", "notes.docx"]).await;
        let mut ends = host
            .open(
                &work.path().join("notes.docx"),
                Viewer {
                    id: "ana".to_owned(),
                    name: "Ana".to_owned(),
                    can_edit: true,
                },
            )
            .await
            .unwrap();
        let join = document::contract::encode_message(&document::ToDocument::Join {
            contract: document::CONTRACT,
            client: "ana".to_owned(),
            epoch: 0,
            rev: 0,
        });
        ends.sender.send(join).await.unwrap();
        let frame = ends.receiver.recv().await.unwrap();
        let decoded = document::contract::decode::<document::ToClient>(&frame).unwrap();
        assert!(matches!(
            decoded,
            document::Frame::Message(document::ToClient::Joined { .. })
        ));
        drop(ends.sender);
        assert!(ends.receiver.recv().await.is_none());
        host.close().await;
    }
}
