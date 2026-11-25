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
