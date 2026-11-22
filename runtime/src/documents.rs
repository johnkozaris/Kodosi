use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use documents::{Documents, Viewer};
use documents_cli::{Agents, Command, formats};
use tokio::sync::mpsc;

use crate::canvas::{CanvasAnswer, PIPE_QUEUE, PipeEnds};
use crate::text::Text;

const STATE_FOLDER: &str = "documents";

#[derive(Parser)]
#[command(name = "kodosi doc", bin_name = "kodosi doc")]
struct DocumentCommand {
    #[command(subcommand)]
    action: DocumentAction,
}

#[derive(Subcommand)]
pub(crate) enum DocumentAction {
    #[command(flatten)]
    Edit(Command),
    /// Draws each page, slide, sheet or screen of text as people see it, and prints the paths of
    /// the pictures.
    Render(RenderArguments),
}

#[derive(Args)]
pub(crate) struct RenderArguments {
    /// A document, sheet, presentation, PDF or text file.
    pub file: PathBuf,
    /// The folder of the pictures. Without it, they go to a private folder of the terminal.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub(crate) fn parse(arguments: Vec<String>) -> Result<DocumentAction, clap::Error> {
    DocumentCommand::try_parse_from(std::iter::once("doc".to_owned()).chain(arguments))
        .map(|command| command.action)
}

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

    pub(crate) async fn open(&self, path: &Path, viewer: Viewer) -> Result<PipeEnds, Text> {
        let pipe = self
            .documents
            .open(path, viewer)
            .await
            .map_err(|error| Text::plain(error.to_string()))?;
        Ok(ends_of(pipe))
    }

    pub(crate) async fn run(&self, viewer: &Viewer, command: Command) -> CanvasAnswer {
        match self.agents.run(viewer, command).await {
            Ok(text) => CanvasAnswer::Text { text, code: 0 },
            Err(error) => CanvasAnswer::Text {
                text: format!("{error}\n"),
                code: 1,
            },
        }
    }

    pub(crate) async fn close(&self) {
        self.documents.close().await;
    }

    pub(crate) async fn close_for_removal(&self, path: &Path) {
        self.documents.close_for_removal(path).await;
    }
}

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
        let Ok(DocumentAction::Edit(command)) = parse(arguments) else {
            panic!("an edit expected: {words:?}");
        };
        match host.run(&agent(), command.rooted(folder)).await {
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
            fresh: true,
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
